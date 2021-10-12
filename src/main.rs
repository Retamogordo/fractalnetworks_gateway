//! # Gateway
//!
//! This crate implements a gateway daemon that manages wireguard connections.
//! It is meant to run on leaf machines and controlled via a centralized
//! manager.
//!
//! It uses [rocket] to server a REST HTTP API with support for HTTP/2. It uses
//! [tokio] as the async runtime, and [sqlx] to talk to a local SQLite database
//! to store traffic data.
//!
//! At runtime, it initialises the database if needed and launches the REST
//! API. When it gets a request to apply some state via the [api::apply]
//! endpoint (via a POST request to `/api/v1/state.json`), it differentially
//! applies that state, meaning that any items (network namespaces, interfaces,
//! networks, peers, addresses, port mappings) that are not in the new config
//! are removed, and new ones are added. Applying the same config twice should
//! not result in any change or disruption to connections.
//!
//! For monitoring purposes, there are two endpoints: the [api::traffic]
//! endpoint allows for monitoring of traffic data for every network, device
//! and the gateway as a whole. Polling this endpoint is recommended. It allows
//! for filtering traffic data by timestamp, such that only newer data is read.

mod api;
mod gateway;
mod token;
mod types;
mod util;
pub mod wireguard;
mod garbage;
mod watchdog;

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
            match watchdog::watchdog(&pool_clone, options.watchdog).await {
                Ok(_) => {}
                Err(e) => log::error!("{}", e),
            }
        }
    });

    // launch garbage collector, which prunes old traffic stats from database
    let pool_clone = pool.clone();
    rocket::tokio::spawn(async move {
        loop {
            match garbage::garbage(&pool_clone, Duration::from_secs(60 * 60)).await {
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
