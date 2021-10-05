mod types;
mod util;
mod api;
pub mod wireguard;
mod gateway;

use anyhow::Result;

#[rocket::main]
async fn main() -> Result<()> {
    rocket::build()
        .mount("/api/v1", api::routes())
        .launch()
        .await?;

    Ok(())
}
