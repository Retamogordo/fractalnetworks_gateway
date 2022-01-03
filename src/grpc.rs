use crate::gateway;
use crate::{Global, Options};
use futures::Stream;
use gateway_client::proto;
use gateway_client::proto::gateway_server::{Gateway, GatewayServer};
use gateway_client::GatewayConfig;
use sqlx::SqlitePool;
use std::pin::Pin;
use tonic::{transport::Server, Request, Response, Status};

impl Global {
    fn check_token(&self, token: &str) -> Result<(), Status> {
        if token != self.options.secret {
            return Err(Status::permission_denied("Invalid token"));
        }
        Ok(())
    }
}

#[tonic::async_trait]
impl Gateway for Global {
    async fn apply(
        &self,
        request: Request<proto::ApplyRequest>,
    ) -> Result<Response<proto::ApplyResponse>, Status> {
        let apply_request = request.into_inner();
        self.check_token(&apply_request.token)?;

        let gateway_config: GatewayConfig = match serde_json::from_str(&apply_request.config) {
            Ok(config) => config,
            Err(e) => return Err(Status::invalid_argument(e.to_string())),
        };

        match gateway::apply(&self, &gateway_config).await {
            Ok(_) => Ok(Response::new(proto::ApplyResponse {
                success: true,
                error_kind: None,
                error_mesg: None,
            })),
            Err(e) => Err(Status::internal(e.to_string())),
        }
    }

    type TrafficStream = Pin<Box<dyn Stream<Item = Result<proto::TrafficResponse, Status>> + Send>>;

    async fn traffic(
        &self,
        request: Request<proto::TrafficRequest>,
    ) -> Result<Response<Self::TrafficStream>, Status> {
        let traffic_request = request.into_inner();
        self.check_token(&traffic_request.token)?;
        unimplemented!()
    }

    type StateStream = Pin<Box<dyn Stream<Item = Result<proto::StateResponse, Status>> + Send>>;

    async fn state(
        &self,
        request: Request<proto::StateRequest>,
    ) -> Result<Response<Self::StateStream>, Status> {
        let state_request = request.into_inner();
        self.check_token(&state_request.token)?;
        unimplemented!()
    }
}

pub async fn run(options: &Options) -> Result<(), anyhow::Error> {
    let global = options.global().await?;
    global.watchdog().await;
    global.garbage().await;
    gateway::startup(&global.options).await?;

    Server::builder()
        .add_service(GatewayServer::new(global))
        .serve("0.0.0.0:9090".parse().unwrap())
        .await?;

    Ok(())
}
