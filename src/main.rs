use anyhow::{Context, Result};
use fractal_gateway::*;
use log::*;
use structopt::StructOpt;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();

    // log name and version on startup.
    info!(
        "Starting {}, version {}",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );

    let global = options.global().await.context("Creating global options")?;

    global.watchdog().await;

    // on startup, initialize nginx and set some default options (such as
    // special redirects passed in on the command line).
    gateway::startup(&options)
        .await
        .context("Starting up gateway")?;

    // connect to the websocket to get config from manager and send events
    // and traffic data
    websocket::connect(global).await;

    Ok(())
}
