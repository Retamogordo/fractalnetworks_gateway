use crate::gateway;
use crate::{Global, Options};
use futures::Stream;
use futures::StreamExt;
use gateway_client::proto;
use gateway_client::proto::gateway_server::{Gateway, GatewayServer};
use gateway_client::GatewayConfig;
use sqlx::SqlitePool;
use std::pin::Pin;
use std::net::SocketAddr;
use tokio_stream::wrappers::BroadcastStream;
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
        let receiver = self.traffic_broadcast.subscribe();
        let stream = BroadcastStream::new(receiver).filter_map(|traffic| async move {
            match traffic {
                Ok(traffic) => Some(Ok(proto::TrafficResponse {
                    traffic: serde_json::to_string(&traffic).unwrap(),
                })),
                Err(_) => None,
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }

    type EventsStream = Pin<Box<dyn Stream<Item = Result<proto::EventsResponse, Status>> + Send>>;

    async fn events(
        &self,
        request: Request<proto::EventsRequest>,
    ) -> Result<Response<Self::EventsStream>, Status> {
        let state_request = request.into_inner();
        self.check_token(&state_request.token)?;
        let subscription = self.events_broadcast.subscribe();
        let stream = BroadcastStream::new(subscription).filter_map(|event| async move {
            match event {
                Ok(event) => Some(Ok(proto::EventsResponse {
                    event: serde_json::to_string(&event).unwrap(),
                })),
                Err(_) => None,
            }
        });
        Ok(Response::new(Box::pin(stream)))
    }
}

pub async fn run(global: Global, grpc_listen: SocketAddr) -> Result<(), anyhow::Error> {
    Server::builder()
        .add_service(GatewayServer::new(global))
        .serve(grpc_listen)
        .await?;

    Ok(())
}
