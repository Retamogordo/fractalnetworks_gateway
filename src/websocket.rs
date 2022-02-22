use crate::Global;
use anyhow::Result;
use log::*;
use std::time::Duration;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;

pub async fn connect(global: Global) {
    loop {
        // try connecting to websocket
        match connect_run(&global).await {
            Ok(()) => {}
            Err(e) => error!("Error connecting to websocket: {}", e),
        };

        // wait some time to reconnect
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn connect_run(global: &Global) -> Result<()> {
    let request = Request::builder()
        .uri(global.manager.to_string())
        .header("Authorization", &format!("Bearer {}", global.token))
        .body(())
        .unwrap()
        .into_client_request()?;

    let socket = connect_async(request).await?;
    info!("Connected to websocket at {}", global.manager);

    Ok(())
}
