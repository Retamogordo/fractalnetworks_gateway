use structopt::StructOpt;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use anyhow::{anyhow, Result};
use log::info;

#[derive(StructOpt, Clone, Debug)]
pub struct Options {
    #[structopt(long, short, default_value = "0.0.0.0:8000", env = "INTEGRATION_LISTEN")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    let options = Options::from_args();
    let socket = TcpListener::bind(&options.listen).await?;
    let (stream, addr) = socket.accept().await?;
    info!("Got gateway connection from {addr}");

    Ok(())

}
