use crate::Global;
use anyhow::Result;
use futures::StreamExt;
use log::*;
use std::time::Duration;
use tokio::select;
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

    let (mut socket, _response) = connect_async(request).await?;
    info!("Connected to websocket at {}", global.manager);

    let mut traffic_sub = global.traffic_broadcast.subscribe();
    let mut events_sub = global.events_broadcast.subscribe();

    loop {
        select! {
            message = socket.next() => {
                match message {
                    Some(Ok(message)) => {}
                    _ => break,
                }
            },
            traffic = traffic_sub.recv() => {
            }
            event = events_sub.recv() => {
            }
        }
    }

    Ok(())
}
