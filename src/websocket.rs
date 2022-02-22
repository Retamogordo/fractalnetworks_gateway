use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use crate::Global;
use anyhow::Result;

pub async fn connect(global: Global) -> Result<()> {
    let request = Request::builder()
        .uri("https://manager.com")
        .header("Authorization", "abc")
        .body(())
        .unwrap()
        .into_client_request();

    Ok(())
}
