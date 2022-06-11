use crate::Global;
use anyhow::{anyhow, Result};
use async_tungstenite::tokio::*;
use async_tungstenite::tungstenite::handshake::client::Request;
use async_tungstenite::tungstenite::Message;
use futures::{SinkExt, StreamExt};
use gateway_client::{GatewayRequest, GatewayResponse};
use log::*;
use serde_json::{from_str, to_string};
use std::time::Duration;
use tokio::select;

pub async fn connect(global: Global) {
    info!("Connecting to manager at {}", global.manager);
    loop {
        // try connecting to websocket
        match connect_run(&global).await {
            Ok(()) => break,
            Err(e) => error!("Error connecting to websocket: {}", e),
        };

        // wait some time to reconnect
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

pub async fn connect_run(global: &Global) -> Result<()> {
    let request = Request::get(&global.manager.to_string())
        .header("Authorization", &format!("Bearer {}", global.token))
        .header("Identity", &global.options.identity)
        .body(())?;

    let (mut socket, _response) = connect_async_with_tls_connector(request, None).await?;
    info!("Connected to websocket at {}", global.manager);

    let mut traffic_sub = global.traffic_broadcast.subscribe();
    let mut events_sub = global.events_broadcast.subscribe();

    loop {
        select! {
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Text(text))) => {
                        let message: GatewayRequest = from_str(&text)?;
                        match message {
                            GatewayRequest::Apply(config) => {
                                crate::gateway::apply(global, &config).await?;
                                socket.send(Message::Text(serde_json::to_string(&GatewayResponse::Apply(Ok(String::new())))?)).await?;
                            },
                            GatewayRequest::ApplyPartial(_config) => {
                            },
                            GatewayRequest::Shutdown => {
                                error!("Received Shutdown message, shutting down");
                                break;
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => return Err(anyhow!("Server closed WebSocket stream")),
                }
            },
            traffic = traffic_sub.recv() => {
                let traffic = traffic?;
                let message = GatewayResponse::Traffic(traffic);
                let message = to_string(&message)?;
                socket.send(Message::Text(message)).await?;
            }
            event = events_sub.recv() => {
                let event = event?;
                let message = GatewayResponse::Event(event);
                let message = to_string(&message)?;
                socket.send(Message::Text(message)).await?;
            }
        }
    }

    Ok(())
}
