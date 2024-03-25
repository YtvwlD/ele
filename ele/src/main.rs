use std::{env, io::IsTerminal, os::fd::{AsFd, AsRawFd}};

use argh::{from_env, FromArgs};
use log::debug;
use pty_process::Pty;
use tokio::io::{copy_bidirectional, join, stdin, stdout};
use zbus::{proxy, zvariant::OwnedFd, Connection, Result};

#[derive(Debug, FromArgs)]
/// Top-level command.
struct Cli {
    /// what user to run the program as
    #[argh(option, default = "\"root\".to_string()")]
    user: String,

    /// the appliation to run
    #[argh(positional)]
    program: String,

    /// the arguments to pass to it
    #[argh(positional, greedy)]
    args: Vec<String>,
}

#[proxy(
    interface = "de.ytvwld.Ele1",
    default_service = "de.ytvwld.Ele",
    default_path = "/de/ytvwld/Ele"
)]
trait EleD {
    async fn create(&self, user: &str, argv: Vec<String>) -> Result<String>;
}

#[proxy(
    interface = "de.ytvwld.Ele1.Process",
    default_service = "de.ytvwld.Ele",
)]
trait EleProcess {
    async fn spawn(&self) -> Result<OwnedFd>;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    env_logger::init();
    let cli: Cli = from_env();
    debug!("Establishing connection to dbus...");
    let connection = Connection::system().await?;
    let eled_proxy = EleDProxy::new(&connection).await?;
    debug!("Waiting for authorization...");
    let mut args = cli.args.clone();
    args.insert(0, cli.program);
    let path = eled_proxy.create(&cli.user, args).await?;
    let process = EleProcessProxy::builder(&connection)
        .path(path)?
        .build().await?;
    // TODO: environment, cwd and resize
    debug!("Spawning process...");
    let fd = process.spawn().await?;
    assert!(fd.as_fd().is_terminal());
    let mut pty = unsafe { Pty::from_raw_fd(fd.as_raw_fd()).unwrap() };
    let mut stdin = stdin();
    let mut stdout = stdout();
    let mut terminal = join(&mut stdin, &mut stdout);
    copy_bidirectional(&mut pty, &mut terminal).await?;

    Ok(())
}
