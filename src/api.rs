use crate::gateway;
use crate::types::*;
use crate::token::Token;
use rocket::serde::json::Json;
use rocket::*;
use std::collections::BTreeMap;
use sqlx::SqlitePool;

#[post("/config.json", data = "<data>")]
async fn config_set(token: Token, data: Json<BTreeMap<u16, NetworkState>>) -> String {
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
async fn config_get(token: Token) -> String {
    "TODO".to_string()
}

#[get("/status.json")]
async fn status(token: Token) -> String {
    "TODO".to_string()
}

#[get("/traffic.json?<start>&<stop>")]
async fn traffic(token: Token, pool: &State<SqlitePool>, start: usize, stop: Option<usize>) -> Json<TrafficInfo> {
    let traffic = gateway::traffic(pool.clone(), start).await.unwrap();
    Json(traffic)
}

pub fn routes() -> Vec<rocket::Route> {
    routes![status, config_get, config_set, traffic]
}
