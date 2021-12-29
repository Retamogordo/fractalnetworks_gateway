use futures::Stream;
use gateway_client::proto;
use gateway_client::proto::gateway_server::{Gateway, GatewayServer};
use sqlx::SqlitePool;
use std::pin::Pin;
use tonic::{transport::Server, Request, Response, Status};

pub struct Service {
    options: crate::Options,
    db: SqlitePool,
}

#[tonic::async_trait]
impl Gateway for Service {
    async fn apply(
        &self,
        request: Request<proto::ApplyRequest>,
    ) -> Result<Response<proto::ApplyResponse>, Status> {
        /*
        let config = request.into_inner();
        let config: gateway_client::GatewayConfig = config.try_into().unwrap();
        crate::gateway::apply(&config, &self.options).await.unwrap();
        Ok(Response::new(proto::ConfigResponse {
            success: true,
            error_kind: None,
            error_mesg: None,
        }))
        */
        unimplemented!()
    }

    type TrafficStream = Pin<Box<dyn Stream<Item = Result<proto::TrafficResponse, Status>> + Send>>;

    async fn traffic(
        &self,
        request: Request<proto::TrafficRequest>,
    ) -> Result<Response<Self::TrafficStream>, Status> {
        unimplemented!()
    }

    type StateStream = Pin<Box<dyn Stream<Item = Result<proto::StateResponse, Status>> + Send>>;

    async fn state(
        &self,
        request: Request<proto::StateRequest>,
    ) -> Result<Response<Self::StateStream>, Status> {
        unimplemented!()
    }
}

pub async fn run(options: &crate::Options) -> Result<(), anyhow::Error> {
    let pool = SqlitePool::connect(&options.database.as_deref().unwrap()).await?;
    sqlx::migrate!().run(&pool).await?;

    let service = Service {
        db: pool,
        options: options.clone(),
    };

    Server::builder()
        .add_service(GatewayServer::new(service))
        .serve("0.0.0.0:9090".parse().unwrap())
        .await?;

    Ok(())
}
