use zbus::{Connection, Result, proxy};

#[proxy(
    interface = "de.ytvwld.Ele1",
    default_service = "de.ytvwld.Ele",
    default_path = "/de/ytvwld/Ele"
)]
trait EleD {
    async fn spawn(&self, argv: Vec<&str>) -> Result<String>;
}

#[async_std::main]
async fn main() -> Result<()> {
    let connection = Connection::session().await?;
    let proxy = EleDProxy::new(&connection).await?;
    let reply = proxy.spawn(vec!["id"]).await?;
    println!("{reply}");

    Ok(())
}
