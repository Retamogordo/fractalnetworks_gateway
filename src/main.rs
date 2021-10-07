mod api;
mod gateway;
mod types;
mod util;
mod token;
pub mod wireguard;

use anyhow::Result;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::time::Duration;
use structopt::StructOpt;
use token::Token;

#[derive(StructOpt, Clone, Debug)]
struct Options {
    #[structopt(long, short, default_value = ":memory:")]
    /// What database file to use to log traffic data to.
    database: String,
    #[structopt(long, short)]
    /// Security token used to authenticate API requests.
    secret: String,
}

#[rocket::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();

    // connect and migrate database
    let pool = SqlitePool::connect(&options.database).await?;
    sqlx::migrate!().run(&pool).await?;

    // launch watchdog, which after the interval will pull in traffic stats
    // and make sure that everything is running as it should.
    rocket::tokio::spawn(gateway::watchdog(Duration::from_secs(60)));

    // launch REST API
    rocket::build()
        .mount("/api/v1", api::routes())
        .manage(Token::new(&options.secret))
        .launch()
        .await?;

    Ok(())
}
