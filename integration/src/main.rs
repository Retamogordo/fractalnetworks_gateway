use anyhow::{anyhow, Result};
use futures::{SinkExt, StreamExt};
use gateway_client::*;
use ipnet::{IpAdd, IpNet};
use log::info;
use rand::{thread_rng, Rng};
use std::net::IpAddr;
use std::net::SocketAddr;
use std::ops::Range;
use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message, WebSocketStream};
use wireguard_keys::{Privkey, Pubkey, Secret};

#[derive(StructOpt, Clone, Debug)]
pub struct Options {
    #[structopt(
        long,
        short,
        default_value = "0.0.0.0:8000",
        env = "INTEGRATION_LISTEN"
    )]
    listen: SocketAddr,
}

const PORT_RANGE: Range<u16> = 50000..60000;
const NETWORK_MTU: usize = 1420;

fn generate_config(size: usize, peers: Range<usize>) -> GatewayConfig {
    let mut config = GatewayConfig::default();
    let mut rng = thread_rng();

    for _ in 0..size {
        let port = rng.gen_range(PORT_RANGE);
        let peers = rng.gen_range(peers.clone());
        let address: IpNet = "10.0.0.1/8".parse().unwrap();
        let mut network = NetworkState {
            private_key: Privkey::generate(),
            listen_port: port,
            mtu: NETWORK_MTU,
            address: vec!["10.0.0.1/8".parse().unwrap()],
            peers: Default::default(),
            proxy: Default::default(),
        };
        for n in 0..peers {
            let address = match address.addr() {
                IpAddr::V4(ipv4) => IpAddr::V4(ipv4.saturating_add(1 + n as u32)),
                IpAddr::V6(ipv6) => IpAddr::V6(ipv6.saturating_add(1 + n as u128)),
            };
            let address = IpNet::new(address, 32).unwrap();
            network.peers.insert(
                Privkey::generate().pubkey(),
                PeerState {
                    allowed_ips: vec![address],
                    endpoint: None,
                    preshared_key: None,
                },
            );
        }

        config.insert(port, network);
    }

    config
}

fn generate_partial_config(size: usize, peers: Range<usize>) -> GatewayConfigPartial {
    let mut config = GatewayConfigPartial::default();

    for (port, network) in generate_config(size, peers).iter() {
        config.insert(*port, Some(network.clone()));
    }

    config
}

async fn apply_config(
    websocket: &mut WebSocketStream<TcpStream>,
    config: GatewayConfig,
) -> Result<Result<String, String>> {
    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Apply(config),
        )?))
        .await?;
    while let Some(Ok(message)) = websocket.next().await {
        match message {
            Message::Text(value) => {
                let value = serde_json::from_str(&value)?;
                match value {
                    GatewayResponse::Apply(status) => {
                        return Ok(status);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Err(anyhow!("Missing apply config response"))
}

async fn apply_partial_config(
    websocket: &mut WebSocketStream<TcpStream>,
    config: GatewayConfigPartial,
) -> Result<Result<String, String>> {
    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::ApplyPartial(config),
        )?))
        .await?;
    while let Some(Ok(message)) = websocket.next().await {
        match message {
            Message::Text(value) => {
                let value = serde_json::from_str(&value)?;
                match value {
                    GatewayResponse::Apply(status) => {
                        return Ok(status);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Err(anyhow!("Missing apply config response"))
}
async fn run_tests(websocket: &mut WebSocketStream<TcpStream>) -> Result<()> {
    info!("Applying empty config");
    let response = apply_config(websocket, Default::default()).await?;
    assert!(response.is_ok());

    for _ in 0..10 {
        info!("Applying config with 10 networks");
        let response = apply_config(websocket, generate_config(10, 0..3)).await?;
        assert!(response.is_ok());
    }

    for _ in 0..10 {
        info!("Applying partial config with 10 networks");
        let response = apply_partial_config(websocket, generate_partial_config(10, 0..3)).await?;
        assert!(response.is_ok());
    }

    info!("Applying empty config");
    let response = apply_config(websocket, Default::default()).await?;
    assert!(response.is_ok());

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();
    let socket = TcpListener::bind(&options.listen).await?;
    let (stream, addr) = socket.accept().await?;
    info!("Got gateway connection from {addr}");
    let mut websocket = accept_async(stream).await?;

    let result = run_tests(&mut websocket).await;
    let _ = websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Shutdown,
        )?))
        .await;

    result
}
