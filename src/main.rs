mod api;
mod gateway;
mod types;
mod util;
pub mod wireguard;

use anyhow::Result;
use std::time::Duration;

#[rocket::main]
async fn main() -> Result<()> {
    rocket::tokio::spawn(gateway::watchdog(Duration::from_secs(60)));

    rocket::build()
        .mount("/api/v1", api::routes())
        .launch()
        .await?;

    Ok(())
}
