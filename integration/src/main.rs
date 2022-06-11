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
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message};
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
        let mut address: IpNet = "10.0.0.1/8".parse().unwrap();
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

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();
    let socket = TcpListener::bind(&options.listen).await?;
    let (stream, addr) = socket.accept().await?;
    info!("Got gateway connection from {addr}");
    let mut websocket = accept_async(stream).await?;

    //
    info!("Applying empty config");
    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Apply(Default::default()),
        )?))
        .await?;
    let event = websocket.next().await.unwrap()?;

    info!("Applying config with 10 networks");
    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Apply(generate_config(10, 0..3)),
        )?))
        .await?;
    let event = websocket.next().await.unwrap()?;

    info!("Applying config with 100 networks");
    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Apply(generate_config(100, 0..3)),
        )?))
        .await?;
    let event = websocket.next().await.unwrap()?;

    websocket
        .send(Message::Text(serde_json::to_string(
            &GatewayRequest::Shutdown,
        )?))
        .await?;

    Ok(())
}
