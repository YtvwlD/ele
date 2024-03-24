use zbus::{Connection, Result, proxy};

#[proxy(
    interface = "de.ytvwld.Ele1",
    default_service = "de.ytvwld.Ele",
    default_path = "/de/ytvwld/Ele"
)]
trait EleD {
    async fn create(&self, user: &str, argv: Vec<&str>) -> Result<String>;
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let connection = Connection::session().await?;
    let proxy = EleDProxy::new(&connection).await?;
    let reply = proxy.create("root", vec!["id"]).await?;
    println!("{reply}");

    Ok(())
}
