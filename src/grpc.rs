use gateway_client::proto;
use gateway_client::proto::gateway_server::{Gateway, GatewayServer};
use sqlx::SqlitePool;
use tonic::{transport::Server, Request, Response, Status};

pub struct Service {
    options: crate::Options,
    db: SqlitePool,
}

#[tonic::async_trait]
impl Gateway for Service {
    async fn apply_config(
        &self,
        request: Request<proto::GatewayConfig>,
    ) -> Result<Response<proto::ConfigResponse>, Status> {
        let config = request.into_inner();
        let config: gateway_client::GatewayConfig = config.try_into().unwrap();
        crate::gateway::apply(&config, &self.options).await.unwrap();
        Ok(Response::new(proto::ConfigResponse {
            success: true,
            error_kind: None,
            error_mesg: None,
        }))
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
