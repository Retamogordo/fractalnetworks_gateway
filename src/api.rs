use crate::gateway;
use crate::types::*;
use rocket::serde::json::Json;
use rocket::*;

#[post("/networks/create", data = "<data>")]
async fn networks_create(data: Json<NetworkState>) -> String {
    gateway::create(&data).await.unwrap()
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
    routes![networks, networks_create, network_get]
}
