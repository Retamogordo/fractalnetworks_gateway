use crate::Global;
use anyhow::{Context, Result};
use gateway_manager_client::proto::gateway_manager_client::*;
use gateway_manager_client::proto::ConfigRequest;
use log::*;
use tonic::Request;
use url::Url;

pub async fn connect(global: Global, manager: Url) {
    info!("Connecting to manager at {}", manager);
    loop {
        match connect_run(global.clone(), manager.clone()).await {
            Err(e) => error!("Error connecting to manager: {}", e),
            _ => {}
        }
    }
}

pub async fn connect_run(global: Global, manager: Url) -> Result<()> {
    let mut client = GatewayManagerClient::connect(manager.to_string())
        .await
        .context("Connecting to Gateway via gRPC")?;
    let response = client
        .config(Request::new(ConfigRequest {
            token: "".to_string(),
        }))
        .await?;

    Ok(())
}
