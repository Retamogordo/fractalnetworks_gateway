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
mod garbage;
mod gateway;
mod token;
mod types;
mod util;
mod watchdog;
pub mod wireguard;

use anyhow::Result;
use sqlx::SqlitePool;
use std::path::Path;
use std::time::Duration;
use structopt::StructOpt;
use token::Token;
use tokio::fs::File;

#[derive(StructOpt, Clone, Debug)]
struct Options {
    /// What database file to use to log traffic data to.
    #[structopt(long, short)]
    database: Option<String>,

    /// Security token used to authenticate API requests.
    #[structopt(long, short)]
    secret: String,

    /// Interval to run watchdog at.
    #[structopt(long, short, default_value="60s", parse(try_from_str = parse_duration::parse::parse))]
    watchdog: Duration,

    /// Interval to run garbage collection at.
    #[structopt(long, short, default_value="1h", parse(try_from_str = parse_duration::parse::parse))]
    garbage: Duration,

    /// Duration for which network data is retained.
    #[structopt(long, short, default_value="24h", parse(try_from_str = parse_duration::parse::parse))]
    retention: Duration,
}

#[rocket::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();

    // create database if not exists
    if let Some(database) = &options.database {
        let database = Path::new(&database);
        if !database.exists() {
            File::create(database).await?;
        }
    }

    let database_string = options.database.unwrap_or_else(|| ":memory:".to_string());

    // connect and migrate database
    let pool = SqlitePool::connect(&database_string).await?;
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
            match garbage::garbage(&pool_clone, options.garbage, options.retention).await {
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
