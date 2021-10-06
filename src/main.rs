mod api;
mod gateway;
mod types;
mod util;
pub mod wireguard;

use anyhow::Result;
use std::time::Duration;

#[rocket::main]
async fn main() -> Result<()> {
    // launch watchdog, which after the interval will pull in traffic stats
    // and make sure that everything is running as it should.
    rocket::tokio::spawn(gateway::watchdog(Duration::from_secs(60)));

    // launch REST API
    rocket::build()
        .mount("/api/v1", api::routes())
        .launch()
        .await?;

    Ok(())
}
