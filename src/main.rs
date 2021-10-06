mod api;
mod gateway;
mod types;
mod util;
pub mod wireguard;

use anyhow::Result;
use std::time::Duration;
use structopt::StructOpt;
use std::path::PathBuf;
use sqlx::SqlitePool;

#[derive(StructOpt, Clone, Debug)]
struct Options {
    #[structopt(long, short, default_value = ":memory:")]
    database: String,
    #[structopt(long, short)]
    secret: String,
}

#[rocket::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();

    // connect and migrate database
    let pool = SqlitePool::connect(&options.database).await?;
    sqlx::migrate!()
        .run(&pool)
        .await?;

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
