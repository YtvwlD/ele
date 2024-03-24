use std::{io::IsTerminal, os::fd::{AsFd, AsRawFd, FromRawFd}};

use tokio::{fs::File, io::{AsyncReadExt, AsyncWriteExt}};
use zbus::{proxy, zvariant::OwnedFd, Connection, Result};

#[proxy(
    interface = "de.ytvwld.Ele1",
    default_service = "de.ytvwld.Ele",
    default_path = "/de/ytvwld/Ele"
)]
trait EleD {
    async fn create(&self, user: &str, argv: Vec<&str>) -> Result<String>;
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
    let connection = Connection::session().await?;
    let eled_proxy = EleDProxy::new(&connection).await?;
    let path = eled_proxy.create("root", vec!["id"]).await?;
    let process = EleProcessProxy::builder(&connection)
        .path(path)?
        .build().await?;
    // TODO: environment, cwd and resize
    let fd = process.spawn().await?;
    assert!(fd.as_fd().is_terminal());
    let mut file = unsafe { File::from_raw_fd(fd.as_raw_fd()) };
    loop {
        let mut buf = [0; 256];
        match file.read(&mut buf).await {
            Ok(0) => todo!("process has closed stdout"),
            Ok(_) => Ok(()),
            Err(e) => match e.kind() {
                // this is fine; we just didn't get any text yet
                std::io::ErrorKind::WouldBlock => Ok(()),
                _ => Err(e),
            }
        }?;
        tokio::io::stdout().write(&buf).await?;
    }
    todo!();

    Ok(())
}
