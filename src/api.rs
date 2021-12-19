use crate::gateway;
use crate::token::Token;
use gateway_client::{GatewayConfig, TrafficInfo};
#[cfg(feature = "openapi")]
use okapi::openapi3::OpenApi;
use rocket::serde::json::Json;
use rocket::*;
#[cfg(feature = "openapi")]
use rocket_okapi::{openapi, openapi_get_routes as routes, openapi_get_spec};
use sqlx::SqlitePool;

#[cfg_attr(feature = "openapi", openapi)]
#[post("/config.json", data = "<data>")]
async fn config_set(_token: Token, data: Json<GatewayConfig>) -> String {
    gateway::apply(&data).await.unwrap()
}

#[cfg_attr(feature = "openapi", openapi)]
#[get("/config.json")]
async fn config_get(_token: Token) -> String {
    "TODO".to_string()
}

#[cfg_attr(feature = "openapi", openapi)]
#[get("/status.json")]
async fn status(_token: Token) -> String {
    "TODO".to_string()
}

#[cfg_attr(feature = "openapi", openapi)]
#[get("/traffic.json?<start>")]
async fn traffic(_token: Token, pool: &State<SqlitePool>, start: usize) -> Json<TrafficInfo> {
    let traffic = gateway::traffic(pool, start).await.unwrap();
    Json(traffic)
}

pub fn routes() -> Vec<rocket::Route> {
    routes![status, config_get, config_set, traffic]
}

#[cfg(feature = "openapi")]
pub fn openapi_json() -> OpenApi {
    openapi_get_spec![status, config_get, config_set, traffic]
}
