use std::sync::Arc;
use deadpool_redis::Pool as RedisPool;
use sqlx::PgPool;
use crate::config::PaymentServiceConfig;


#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub redis: RedisPool,
    pub config: Arc<PaymentServiceConfig>,
    pub http_client: reqwest::Client,
}

impl AppState {
    pub fn new(db: PgPool, redis: RedisPool, config: PaymentServiceConfig) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .user_agent(format!("rustpay/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            db,
            redis,
            config: Arc::new(config),
            http_client,
        }
    }
}
