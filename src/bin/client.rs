use anyhow::Result;
use async_trait::async_trait;
use gateway_client::{GatewayClient, GatewayConfig, TrafficInfo};
use reqwest::{Client, ClientBuilder};
use serde_json::to_string_pretty;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use url::Url;

#[derive(StructOpt, Debug, Clone)]
pub struct Options {
    /// Url to the server running the storage API.
    #[structopt(long, short)]
    api: Url,
    /// Allow invalid TLS certificates.
    #[structopt(long)]
    insecure: bool,
    /// Token used to authenticate to gateway manager.
    #[structopt(long, short)]
    token: String,
    /// Subcommand to run.
    #[structopt(subcommand)]
    command: Command,
}

impl Options {
    pub fn client(&self) -> Result<Client> {
        Ok(ClientBuilder::new()
            .danger_accept_invalid_certs(self.insecure)
            .build()?)
    }
}

#[async_trait]
trait Runnable {
    async fn run(self, options: &Options) -> Result<()>;
}

#[derive(StructOpt, Debug, Clone)]
pub enum Command {
    /// Manage gateways connected to this manager.
    ConfigSet(ConfigSetCommand),
    /// Commands related to managing networks.
    Traffic(TrafficCommand),
}

#[async_trait]
impl Runnable for Command {
    async fn run(self, options: &Options) -> Result<()> {
        use Command::*;
        match self {
            ConfigSet(command) => command.run(options).await,
            Traffic(command) => command.run(options).await,
        }
    }
}

#[derive(StructOpt, Debug, Clone)]
pub struct ConfigSetCommand {
    config: PathBuf,
}

#[async_trait]
impl Runnable for ConfigSetCommand {
    async fn run(self, options: &Options) -> Result<()> {
        let client = options.client()?;
        let mut file = File::open(&self.config).await?;
        let mut contents = vec![];
        file.read_to_end(&mut contents).await?;
        let config = serde_json::from_slice(&contents)?;
        options
            .api
            .config_set(&client, &options.token, &config)
            .await?;
        Ok(())
    }
}

#[derive(StructOpt, Debug, Clone)]
pub struct TrafficCommand {}

#[async_trait]
impl Runnable for TrafficCommand {
    async fn run(self, options: &Options) -> Result<()> {
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let options = Options::from_args();
    match options.command.clone().run(&options).await {
        Ok(_) => {}
        Err(e) => eprintln!("{}", e),
    }
}
