#[macro_use]
extern crate rocket;
use anyhow::Result;

mod api;
mod wireguard;
mod gateway;

#[get("/networks")]
async fn networks() -> &'static str {
    "Hello, world"
}

#[rocket::main]
async fn main() -> Result<()> {
    rocket::build()
        .mount("/api/v1", api::routes())
        .launch()
        .await?;

    Ok(())
}
