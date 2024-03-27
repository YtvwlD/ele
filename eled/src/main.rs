use std::{collections::HashMap, env, error, ffi::OsStr, ops::Deref, os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd}, path::Path, process::Stdio, time::Duration};
use log::{debug, error, trace};
use nix::{sys::signal::{kill, Signal}, unistd::Pid};
use pty_process::Pty;
use tokio::{process::Child, sync::RwLock, time::sleep};
use zbus::{connection, fdo::Error, interface, message::Header, object_server::InterfaceRef, zvariant::Fd, Connection, ObjectServer};
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

static PROCESS_IDS: RwLock<Vec<usize>> = RwLock::const_new(Vec::new());

struct EleD {
    /// The ID to give to the next spawned process.
    /// Note that this being integers is an implementation detail:
    /// Clients should treat this as an opaque string.
    next_id: usize,
}

impl EleD {
    /// Creates a new instance of the main object.
    fn new() -> Self {
        Self { next_id: 1 }
    }

    async fn check_authorization(connection: &Connection, header: &Header<'_>) -> Result<(), Error> {
        debug!("checking authorization...");
        let polkit = AuthorityProxy::new(&connection).await?;
        let subject = Subject::new_for_message_header(header)
            .map_err( |e| match e {
                zbus_polkit::Error::Io(i) => Error::IOError(i.to_string()),
                zbus_polkit::Error::ParseInt(i) => Error::InvalidArgs(i.to_string()),
                zbus_polkit::Error::BadSender(i) => Error::InconsistentMessage(i.to_string()),
                zbus_polkit::Error::MissingSender => Error::InconsistentMessage("missing sender".to_string()),
                i => Error::AuthFailed(i.to_string()),
            })?;
        let result = polkit.check_authorization(
            &subject,
            "org.freedesktop.policykit.exec", // TODO: use a custom one
            &HashMap::new(),
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        ).await?;
        if result.is_authorized {
            Ok(())
        } else {
            Err(Error::AccessDenied("not authorized".to_string()))
        }
    }
}

#[interface(name = "de.ytvwld.Ele1")]
impl EleD {
    async fn create(
        &mut self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(object_server)] object_server: &&ObjectServer,
        #[zbus(header)] header: Header<'_>,
        user: &str, argv: Vec<&str>, interactive: bool,
    ) -> Result<String, Error> {
        let sender = header
            .sender()
            .ok_or(Error::AccessDenied("couldn't get sender".to_string()))?
            .as_str()
            .to_string();
        debug!("Client {} has asked us to execute {:?} as {}.", sender, argv, user);
        assert_eq!(user, "root"); // TODO
        Self::check_authorization(connection, &header).await?;
        let process = EleProcess::new(sender, argv, interactive)?;
        let id = self.next_id;
        PROCESS_IDS.write().await.push(id);
        self.next_id += 1;
        let path = format!("/de/ytvwld/Ele/{id}");
        debug!("Registering object at {path}...");
        object_server.at(path.clone(), process).await?;
        Ok(path)
    }
}

/// A process that might be running.
/// 
/// All that we know is that the caller has been successfully authenticated
/// to run this process.
struct EleProcess {
    /// the unique name of the client that created this process
    sender: String,
    attached_to: AttachedTo,
    command: Command,
    child: Option<Child>,
}

impl EleProcess {
    /// Create a new process.
    /// 
    /// We *need* to make sure that the caller is authenticated to perform this
    /// action *beforehand*.
    fn new(sender: String, argv: Vec<&str>, interactive: bool) -> Result<Self, Error> {
        let attached_to = AttachedTo::new(interactive)?;
        let mut argv_iter = argv.iter();
        let mut command = Command::new(argv_iter.next().ok_or(
            Error::InvalidArgs("command is missing".to_string())
        )?, interactive);
        command.args(argv_iter);
        Ok(Self { sender, attached_to, command, child: None })
    }

    fn check_caller(&self, header: Header<'_>) -> Result<(), Error> {
        if header.sender().ok_or(
            Error::AccessDenied("couldn't get sender".to_string())
        )?.as_str() == self.sender {
            Ok(())
        } else {
            Err(Error::AccessDenied("this process was created by a different sender".to_string()))
        }
    }

    /// Check whether the child has exited.
    /// 
    /// If it has, close the pty, unregister the dbus object and return true.
    async fn check_exited(&mut self, object_server: &ObjectServer, id: usize) -> Result<bool, Box<dyn error::Error>> {
        // the child can only have exited if it has been started
        if let Some(child) = self.child.as_mut() {
            // let-chains are unstable
            if child.try_wait()?.is_some() {
                debug!("process {id} has exited; closing pty");
                if let AttachedTo::Pty(pty) = &self.attached_to {
                    // dropping a pty doesn't seem to close it?
                    unsafe { OwnedFd::from_raw_fd(pty.as_raw_fd()) };
                    self.attached_to = AttachedTo::Nothing;
                }
                // deregister the whole object
                if matches!(
                    object_server.remove::<EleProcess, _>(format!("/de/ytvwld/Ele/{id}")).await,
                    Ok(true)
                ) {
                    Ok(true)
                } else {
                    error!("failed to unregister process {id}");
                    Err("failed to unregister process")?
                }
            } else {
                Ok(false)
            }
        } else {
            Ok(false)
        }
    }
}

#[interface(name = "de.ytvwld.Ele1.Process")]
impl EleProcess {
    async fn environment(
        &mut self,
        #[zbus(header)] header: Header<'_>,
        environ: HashMap<&str, &str>,
    ) -> Result<(), Error> {
        self.check_caller(header)?;
        if self.child.is_some() {
            return Err(Error::FileExists("can't set environ after the process has been started".to_string()));
        }
        debug!("setting environment...");
        self.command.envs(environ.iter());
        Ok(())
    }

    async fn directory(
        &mut self,
        #[zbus(header)] header: Header<'_>,
        path: &str,
    ) -> Result<(), Error> {
        self.check_caller(header)?;
        if self.child.is_some() {
            return Err(Error::FileExists("can't set cwd after the process has been started".to_string()));
        }
        debug!("setting directory to {path}...");
        self.command.current_dir(path);
        Ok(())
    }

    async fn resize(
        &mut self,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<String, Error> {
        self.check_caller(header)?;
        // TODO: pty.resize
        todo!()
    }

    async fn signal(
        &mut self,
        #[zbus(header)] header: Header<'_>,
        signal: i32,
    ) -> Result<(), Error> {
        self.check_caller(header)?;
        if let Some(child) = &self.child {
            debug!("sending signal {signal} to process...");
            kill(
                Pid::from_raw(i32::try_from(
                    child.id().ok_or(Error::IOError("child process has no pid".to_string()))?
                ).map_err(|e| Error::IOError(e.to_string()))?),
                Signal::try_from(signal).map_err(|e| Error::InvalidArgs(e.to_string()))?
            ).map_err(|e| Error::IOError(e.to_string()))?;
            Ok(())
        } else {
            return Err(Error::FileNotFound("process is not running".to_string()));
        }
    }

    async fn spawn(
        &mut self,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<Vec<Fd>, Error> {
        self.check_caller(header)?;
        if self.child.is_some() {
            return Err(Error::FileExists("process is already running".to_string()));
        }
        debug!("spawning process...");
        self.child = Some(self.command.spawn(&self.attached_to)?);
        Ok(
            self.attached_to
            .fds(self.child.as_mut().unwrap())
            .into_iter()
            .map(|f| Fd::Borrowed(f))
            .collect()
        )
    }
}

/// Something a process can be attached to: a pty for interactive use or
/// pipes for non-interactive use.
enum AttachedTo {
    /// A pseudo-terminal. This is useful for interactive applications.
    Pty(Pty),
    /// stdin, stdout and stderr. This is useful for non-interactive applications.
    Pipes,
    /// Nothing. The application has probably exited.
    Nothing,
}

impl AttachedTo {
    fn new(interactive: bool) -> Result<Self, Error> {
        if interactive {
            debug!("Creating pty...");
            Pty::new()
                .map(Self::Pty)
                .map_err(|e| Error::SpawnFailed(e.to_string()))
        } else {
            Ok(Self::Pipes)
        }
    }

    fn fds<'a>(&'a self, child: &'a mut Child) -> Vec<BorrowedFd<'a>> {
        let mut borrowed = Vec::new();
        match self {
            Self::Pty(pty) => borrowed.push(pty.as_fd()),
            Self::Pipes => {
                borrowed.push(child.stdin.as_ref().unwrap().as_fd());
                borrowed.push(child.stderr.as_ref().unwrap().as_fd());
                borrowed.push(child.stdout.as_ref().unwrap().as_fd());
            },
            Self::Nothing => (),
        }
        borrowed
    }
}

/// A command to be executed.
/// 
/// This abstracts over interactive and non-interactive commands.
enum Command {
    Pty(pty_process::Command),
    Tokio(tokio::process::Command),
}

impl Command {
    fn new<S: AsRef<OsStr>>(program: S, interactive: bool) -> Self {
        match interactive {
            true => Self::Pty(pty_process::Command::new(program)),
            false => Self::Tokio(tokio::process::Command::new(program)),
        }
    }
    
    fn args(&mut self, args: std::slice::Iter<'_, &str>) -> &mut Self {
        match self {
            Command::Pty(p) => { p.args(args); self },
            Command::Tokio(t) => { t.args(args); self },
        }
    }

    fn envs<I, K, V>(&mut self, vars: I) -> &mut Self
    where I: IntoIterator<Item = (K, V)>, K: AsRef<std::ffi::OsStr>, V: AsRef<std::ffi::OsStr> {
        match self {
            Command::Pty(p) => { p.envs(vars); self },
            Command::Tokio(t) => { t.envs(vars); self },
        }
    }

    fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Self {
        match self {
            Command::Pty(p) => { p.current_dir(dir); self },
            Command::Tokio(t) => { t.current_dir(dir); self },
        }
    }

    fn spawn(&mut self, attached_to: &AttachedTo) -> Result<Child, Error> {
        match self {
            Command::Pty(p) => if let AttachedTo::Pty(pty) = attached_to {
                p.spawn(&pty.pts().map_err(
                    |e| Error::SpawnFailed(e.to_string())
                )?).map_err(
                    |e| Error::SpawnFailed(e.to_string())
                )
            } else {
                panic!("pty is missing");
            },
            Command::Tokio(t) => if let AttachedTo::Pipes = attached_to {
                t.stdin(Stdio::piped());
                t.stdout(Stdio::piped());
                t.stderr(Stdio::piped());
                t.spawn().map_err(
                    |e| Error::SpawnFailed(e.to_string())
                )
            } else {
                panic!("pipes are missing");
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn error::Error>> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    env_logger::init();
    debug!("Establishing connection to dbus...");
    let conn = connection::Builder::system()?
        .name("de.ytvwld.Ele")?
        .serve_at("/de/ytvwld/Ele", EleD::new())?
        .build()
        .await?;

    // loop through the processes to see which has stopped
    loop {
        trace!("checking for processes that have exited...");
        let len = PROCESS_IDS.read().await.len();
        for id_idx in 0..len {
            let id = {
                let lock = PROCESS_IDS.read().await;
                *lock.get(id_idx).expect("failed to get process id")
            };
            let process: InterfaceRef<EleProcess> = conn.object_server()
                .interface(format!("/de/ytvwld/Ele/{id}")).await?;
            if process.get_mut().await.check_exited(conn.object_server().deref(), id).await? {
                PROCESS_IDS.write().await.remove(id_idx);
                break;
            };
        }
        sleep(Duration::from_secs(2)).await;
    }
}