mod api;
mod gateway;
mod token;
mod types;
mod util;
pub mod wireguard;

use anyhow::Result;
use sqlx::SqlitePool;
use std::time::Duration;
use structopt::StructOpt;
use token::Token;

#[derive(StructOpt, Clone, Debug)]
struct Options {
    /// What database file to use to log traffic data to.
    #[structopt(long, short, default_value = ":memory:")]
    database: String,

    /// Security token used to authenticate API requests.
    #[structopt(long, short)]
    secret: String,

    /// Interval to run watchdog at.
    #[structopt(long, short, default_value="60s", parse(try_from_str = parse_duration::parse::parse))]
    watchdog: Duration,
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
    let pool_clone = pool.clone();
    rocket::tokio::spawn(async move {
        loop {
            match gateway::watchdog(&pool_clone, options.watchdog).await {
                Ok(_) => {}
                Err(e) => log::error!("{}", e),
            }
        }
    });

    // launch garbage collector, which prunes old traffic stats from database
    let pool_clone = pool.clone();
    rocket::tokio::spawn(async move {
        loop {
            match gateway::garbage(&pool_clone, Duration::from_secs(60 * 60)).await {
                Ok(_) => {}
                Err(e) => log::error!("{}", e),
            }
        }
    });

    // launch REST API
    rocket::build()
        .mount("/api/v1", api::routes())
        .manage(Token::new(&options.secret))
        .manage(pool)
        .launch()
        .await?;

    Ok(())
}
