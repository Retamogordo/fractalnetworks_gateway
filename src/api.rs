use crate::gateway;
use crate::token::Token;
use crate::types::*;
use rocket::serde::json::Json;
use rocket::*;
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use gateway_client::GatewayConfig;

#[post("/config.json", data = "<data>")]
async fn config_set(_token: Token, data: Json<GatewayConfig>) -> String {
    gateway::apply(&data).await.unwrap()
}

#[get("/config.json")]
async fn config_get(_token: Token) -> String {
    "TODO".to_string()
}

#[get("/status.json")]
async fn status(_token: Token) -> String {
    "TODO".to_string()
}

#[get("/traffic.json?<start>")]
async fn traffic(_token: Token, pool: &State<SqlitePool>, start: usize) -> Json<TrafficInfo> {
    let traffic = gateway::traffic(pool, start).await.unwrap();
    Json(traffic)
}

pub fn routes() -> Vec<rocket::Route> {
    routes![status, config_get, config_set, traffic]
}
