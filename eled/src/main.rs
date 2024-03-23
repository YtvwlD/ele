use std::{error::Error, future::pending};
use zbus::{connection, interface};

struct EleD {
}

#[interface(name = "de.ytvwld.Ele1")]
impl EleD {
    async fn spawn(&mut self, argv: Vec<&str>) -> String {
        todo!()
    }
}

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let eled = EleD {};
    let _conn = connection::Builder::session()?
        .name("de.ytvwld.Ele")?
        .serve_at("/de/ytvwld/Ele", eled)?
        .build()
        .await?;

    // wait forever
    pending::<()>().await;

    Ok(())
}