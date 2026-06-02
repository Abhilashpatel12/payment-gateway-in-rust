use std::sync::Arc;

use deadpool_redis::Pool as RedisPool;
use sqlx::PgPool;

use crate::config::GatewayConfig;

#[derive(Clone)]
pub struct GatewayState {
    pub db: PgPool,
    pub redis: RedisPool,
    pub config: Arc<GatewayConfig>,
    pub http_client: reqwest::Client,
}

impl GatewayState {
    pub fn new(db: PgPool, redis: RedisPool, config: GatewayConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent(format!("rustpay-gateway/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("failed to build gateway HTTP client");

        Self {
            db,
            redis,
            config: Arc::new(config),
            http_client,
        }
    }
}
