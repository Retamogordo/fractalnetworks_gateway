use crate::gateway;
use crate::types::*;
use rocket::serde::json::Json;
use rocket::*;
use std::collections::BTreeMap;

#[post("/networks/create", data = "<data>")]
async fn networks_create(data: Json<NetworkState>) -> String {
    gateway::create(&data).await.unwrap()
}

#[post("/apply", data = "<data>")]
async fn apply(data: Json<BTreeMap<u16, NetworkState>>) -> String {
    let data: Vec<NetworkState> = data.iter()
        .map(|(port, state)| {
            let mut state = state.clone();
            state.listen_port = *port;
            state
        })
        .collect();
    gateway::apply(&data).await.unwrap()
}

#[get("/networks")]
async fn networks() -> &'static str {
    "Hello, world"
}

#[get("/network/<public_key>")]
async fn network_get(public_key: &str) -> &'static str {
    "Network"
}

pub fn routes() -> Vec<rocket::Route> {
    routes![networks, networks_create, network_get, apply]
}
