use anyhow::{Context, Result};
use async_trait::async_trait;
use futures::StreamExt;
#[cfg(feature = "proto")]
use gateway_client::proto::{
    gateway_client::GatewayClient as GatewayGrpcClient, ApplyRequest, TrafficRequest,
};
use gateway_client::{GatewayClient, GatewayConfig, TrafficInfo};
use reqwest::{Client, ClientBuilder};
use serde_json::to_string_pretty;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use structopt::StructOpt;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
#[cfg(feature = "proto")]
use tonic::Request;
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
    #[cfg(feature = "proto")]
    #[structopt(long, short)]
    grpc: bool,
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
        let mut file = File::open(&self.config)
            .await
            .context("Opening configuration file")?;
        let mut contents = vec![];
        file.read_to_end(&mut contents)
            .await
            .context("Reading configuration file")?;
        let config =
            serde_json::from_slice(&contents).context("Parsing configuration file JSON")?;

        #[cfg(feature = "proto")]
        if options.grpc {
            let mut client = GatewayGrpcClient::connect(options.api.to_string())
                .await
                .context("Connecting to Gateway via gRPC")?;
            let response = client
                .apply(Request::new(ApplyRequest {
                    token: options.token.clone(),
                    config: serde_json::to_string(&config)?,
                }))
                .await?;
            return Ok(());
        }

        let client = options.client()?;
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
        #[cfg(feature = "proto")]
        if options.grpc {
            let mut client = GatewayGrpcClient::connect(options.api.to_string())
                .await
                .context("Connecting to Gateway via gRPC")?;
            let mut response = client
                .traffic(Request::new(TrafficRequest {
                    token: options.token.clone(),
                }))
                .await?;
            let mut response = response.into_inner();
            while let Some(Ok(traffic)) = response.next().await {
                let data: TrafficInfo = serde_json::from_str(&traffic.traffic)?;
                println!("{}", serde_json::to_string(&data)?);
            }
            return Ok(());
        }

        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let options = Options::from_args();
    match options.command.clone().run(&options).await {
        Ok(_) => {}
        Err(e) => eprintln!("{:?}", e),
    }
}
