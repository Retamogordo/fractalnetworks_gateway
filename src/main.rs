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

use anyhow::{anyhow, Context, Result};
use sqlx::SqlitePool;
use std::net::SocketAddr;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use structopt::StructOpt;
use token::Token;
use tokio::fs::File;
use url::Url;

#[derive(StructOpt, Clone, Debug)]
pub enum Command {
    /// Run gateway.
    Run(Options),
    /// Generate OpenAPI documentation.
    #[cfg(feature = "openapi")]
    Openapi,
    /// Migrate database.
    Migrate {
        /// What database file to use to log traffic data to.
        #[structopt(long, short, env = "GATEWAY_DATABASE")]
        database: String,
    },
}

#[derive(StructOpt, Clone, Debug)]
pub struct Options {
    /// What database file to use to log traffic data to.
    #[structopt(long, short, env = "GATEWAY_DATABASE")]
    database: Option<String>,

    /// Security token used to authenticate API requests.
    #[structopt(long, short, env = "GATEWAY_TOKEN")]
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

    /// Add custom HTTPS forwarding
    #[structopt(long, env = "GATEWAY_CUSTOM_FORWARDING", parse(try_from_str = parse_custom_forwarding), use_delimiter = true)]
    custom_forwarding: Vec<(Url, SocketAddr)>,
}

/// Given a forwarding scheme like `https://domain.com=127.0.0.1:8000`, parse it
/// into URL and SocketAddr.
fn parse_custom_forwarding(text: &str) -> Result<(Url, SocketAddr)> {
    let mut parts = text.split("=");
    let url_part = parts.next().ok_or(anyhow!("Missing URL part"))?;
    let url = Url::parse(url_part).context("While parsing forwarding URL")?;
    let socket_part = parts.next().ok_or(anyhow!("Missing socket part"))?;
    let socket = SocketAddr::from_str(socket_part)?;
    Ok((url, socket))
}

impl Options {
    pub async fn run(self) -> Result<()> {
        // create database if not exists
        if let Some(database) = &self.database {
            let database = Path::new(&database);
            if !database.exists() {
                File::create(database).await?;
            }
        }

        let database_string = self.database.as_deref().unwrap_or_else(|| ":memory:");

        // connect and migrate database
        let pool = SqlitePool::connect(&database_string).await?;
        sqlx::migrate!().run(&pool).await?;

        // launch watchdog, which after the interval will pull in traffic stats
        // and make sure that everything is running as it should.
        let pool_clone = pool.clone();
        rocket::tokio::spawn(async move {
            loop {
                match watchdog::watchdog(&pool_clone, self.watchdog).await {
                    Ok(_) => {}
                    Err(e) => log::error!("{}", e),
                }
            }
        });

        // launch garbage collector, which prunes old traffic stats from database
        let pool_clone = pool.clone();
        rocket::tokio::spawn(async move {
            loop {
                match garbage::garbage(&pool_clone, self.garbage, self.retention).await {
                    Ok(_) => {}
                    Err(e) => log::error!("{}", e),
                }
            }
        });

        gateway::startup(&self).await?;

        // launch REST API
        rocket::build()
            .mount("/api/v1", api::routes())
            .manage(Token::new(&self.secret))
            .manage(pool)
            .manage(self.clone())
            .launch()
            .await?;

        Ok(())
    }
}

#[rocket::main]
async fn main() -> Result<()> {
    env_logger::init();
    let command = Command::from_args();

    match command {
        #[cfg(feature = "openapi")]
        Command::Openapi => {
            let openapi = api::openapi_json();
            println!("{}", serde_json::to_string(&openapi)?);
            Ok(());
        }
        Command::Migrate { database } => {
            let pool = SqlitePool::connect(&database).await?;
            sqlx::migrate!().run(&pool).await?;
            Ok(())
        }
        Command::Run(options) => {
            options.run().await?;
            Ok(())
        }
    }
}
