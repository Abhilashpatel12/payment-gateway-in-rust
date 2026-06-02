use anyhow::{Context, Result};
use common::config::{DatabaseConfig, RedisConfig, ServerConfig, TelemetryConfig};

#[derive(Debug, Clone)]
pub struct GatewayConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub telemetry: TelemetryConfig,
    pub payment_service_url: String,
    pub merchant_service_url: String,
    pub order_service_url: String,
    pub idempotency_ttl_seconds: u64,
}

impl GatewayConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            server: ServerConfig {
                host: std::env::var("API_GATEWAY_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
                port: std::env::var("API_GATEWAY_PORT")
                    .unwrap_or_else(|_| "8080".into())
                    .parse()
                    .context("Invalid API_GATEWAY_PORT")?,
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL").context("DATABASE_URL required")?,
                max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(20),
                min_connections: 2,
                connect_timeout_secs: 10,
            },
            redis: RedisConfig {
                url: std::env::var("REDIS_URL").context("REDIS_URL required")?,
                max_connections: 10,
            },
            telemetry: TelemetryConfig {
                otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:4317".into()),
                service_name: std::env::var("OTEL_SERVICE_NAME")
                    .unwrap_or_else(|_| "api-gateway".into()),
            },
            payment_service_url: std::env::var("PAYMENT_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8081".into()),
            merchant_service_url: std::env::var("MERCHANT_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8082".into()),
            order_service_url: std::env::var("ORDER_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8083".into()),
            idempotency_ttl_seconds: std::env::var("IDEMPOTENCY_TTL_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(86_400),
        })
    }
}
