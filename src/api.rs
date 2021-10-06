use crate::gateway;
use crate::types::*;
use rocket::serde::json::Json;
use rocket::*;
use std::collections::BTreeMap;

#[post("/config.json", data = "<data>")]
async fn config_set(data: Json<BTreeMap<u16, NetworkState>>) -> String {
    let data: Vec<NetworkState> = data
        .iter()
        .map(|(port, state)| {
            let mut state = state.clone();
            state.listen_port = *port;
            state
        })
        .collect();
    gateway::apply(&data).await.unwrap()
}

#[get("/config.json")]
async fn config_get() -> String {
    "TODO".to_string()
}

#[get("/status.json")]
async fn status() -> String {
    "TODO".to_string()
}

#[get("/traffic.json")]
async fn traffic() -> String {
    "TODO".to_string()
}

pub fn routes() -> Vec<rocket::Route> {
    routes![status, config_get, config_set, traffic]
}
