use anyhow::{Context, Result};
use common::config::{DatabaseConfig, KafkaConfig, RedisConfig, ServerConfig, TelemetryConfig};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct PaymentServiceConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub kafka: KafkaConfig,
    pub telemetry: TelemetryConfig,
    pub idempotency_ttl_seconds: u64,
    pub vault_service_url: String,
    pub acquirer_router_url: String,
    pub fraud_service_url: String,
}

impl PaymentServiceConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            server: ServerConfig {
                host: std::env::var("PAYMENT_SERVICE_HOST")
                    .unwrap_or_else(|_| "0.0.0.0".into()),
                port: std::env::var("PAYMENT_SERVICE_PORT")
                    .unwrap_or_else(|_| "8081".into())
                    .parse()
                    .context("Invalid PAYMENT_SERVICE_PORT")?,
            },
            database: DatabaseConfig {
                url: std::env::var("DATABASE_URL").context("DATABASE_URL required")?,
                max_connections: std::env::var("DATABASE_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(20),
                min_connections: std::env::var("DATABASE_MIN_CONNECTIONS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(2),
                connect_timeout_secs: 10,
            },
            redis: RedisConfig {
                url: std::env::var("REDIS_URL").context("REDIS_URL required")?,
                max_connections: 10,
            },
            kafka: KafkaConfig {
                bootstrap_servers: std::env::var("KAFKA_BOOTSTRAP_SERVERS")
                    .unwrap_or_else(|_| "localhost:9092".into()),
                consumer_group_id: std::env::var("KAFKA_CONSUMER_GROUP_ID")
                    .unwrap_or_else(|_| "rustpay-consumers".into()),
                topic_payments: std::env::var("KAFKA_TOPIC_PAYMENTS")
                    .unwrap_or_else(|_| "rustpay.payments".into()),
                topic_merchants: std::env::var("KAFKA_TOPIC_MERCHANTS")
                    .unwrap_or_else(|_| "rustpay.merchants".into()),
                topic_webhooks: std::env::var("KAFKA_TOPIC_WEBHOOKS")
                    .unwrap_or_else(|_| "rustpay.webhooks".into()),
                topic_ledger: std::env::var("KAFKA_TOPIC_LEDGER")
                    .unwrap_or_else(|_| "rustpay.ledger".into()),
                topic_fraud: std::env::var("KAFKA_TOPIC_FRAUD")
                    .unwrap_or_else(|_| "rustpay.fraud".into()),
            },
            telemetry: TelemetryConfig {
                otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                    .unwrap_or_else(|_| "http://localhost:4317".into()),
                service_name: std::env::var("OTEL_SERVICE_NAME")
                    .unwrap_or_else(|_| "payment-service".into()),
            },
            idempotency_ttl_seconds: std::env::var("IDEMPOTENCY_TTL_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(86400),
            vault_service_url: std::env::var("VAULT_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8085".into()),
            acquirer_router_url: std::env::var("ACQUIRER_ROUTER_URL")
                .unwrap_or_else(|_| "http://localhost:8086".into()),
            fraud_service_url: std::env::var("FRAUD_SERVICE_URL")
                .unwrap_or_else(|_| "http://localhost:8087".into()),
        })
    }
}
