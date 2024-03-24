use std::{error, future::pending, os::fd::AsFd};
use pty_process::{Command, Pty};
use tokio::process::Child;
use zbus::{connection, fdo::Error, interface, message::Header, zvariant::Fd, ObjectServer};

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
}

#[interface(name = "de.ytvwld.Ele1")]
impl EleD {
    async fn create(
        &mut self,
        #[zbus(object_server)] object_server: &&ObjectServer,
        #[zbus(header)] header: Header<'_>,
        user: &str, argv: Vec<&str>,
    ) -> Result<String, Error> {
        assert_eq!(user, "root"); // TODO
        // TODO: actually check authentication
        let process = EleProcess::new(
            header
                .sender()
                .ok_or(Error::AccessDenied("couldn't get sender".to_string()))?
                .as_str()
                .to_string(),
            argv,
        )?;
        let id = self.next_id;
        self.next_id += 1;
        let path = format!("/de/ytvwld/Ele/{id}");
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
    pty: Pty,
    command: Command,
    child: Option<Child>,
}

impl EleProcess {
    /// Create a new process.
    /// 
    /// We *need* to make sure that the caller is authenticated to perform this
    /// action *beforehand*.
    fn new(sender: String, argv: Vec<&str>) -> Result<Self, Error> {
        let pty = Pty::new()
            .map_err(|e| Error::SpawnFailed(e.to_string()))?;
        let mut argv_iter = argv.iter();
        let mut command = Command::new(argv_iter.next().ok_or(
            Error::InvalidArgs("command is missing".to_string())
        )?);
        command.args(argv_iter);
        Ok(Self { sender, pty, command, child: None })
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
}

#[interface(name = "de.ytvwld.Ele1.Process")]
impl EleProcess {
    async fn environment(
        &mut self,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<String, Error> {
        self.check_caller(header)?;
        todo!()
    }

    async fn resize(
        &mut self,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<String, Error> {
        self.check_caller(header)?;
        // TODO: pty.resize
        todo!()
    }
        
    async fn spawn(
        &mut self,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<Fd, Error> {
        self.check_caller(header)?;
        if self.child.is_some() {
            return Err(Error::FileExists("process is already running".to_string()));
        }
        self.child = Some(self.command.spawn(&self.pty.pts().map_err(
            |e| Error::SpawnFailed(e.to_string())
        )?).map_err(|e| Error::SpawnFailed(e.to_string()))?);
        Ok(Fd::Borrowed(self.pty.as_fd()))
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let _conn = connection::Builder::session()?
        .name("de.ytvwld.Ele")?
        .serve_at("/de/ytvwld/Ele", EleD::new())?
        .build()
        .await?;

    // wait forever
    pending::<()>().await;

    Ok(())
}