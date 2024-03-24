use std::{io::IsTerminal, os::fd::AsFd};

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
    todo!();

    Ok(())
}
