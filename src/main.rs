mod api;
mod gateway;
mod types;
mod util;
pub mod wireguard;

use anyhow::Result;

#[rocket::main]
async fn main() -> Result<()> {
    rocket::build()
        .mount("/api/v1", api::routes())
        .launch()
        .await?;

    Ok(())
}
