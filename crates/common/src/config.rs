use serde::Deserialize;


#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
}

fn default_max_connections() -> u32 { 20 }
fn default_min_connections() -> u32 { 2 }
fn default_connect_timeout() -> u64 { 10 }

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: String,
    #[serde(default = "default_redis_max")]
    pub max_connections: usize,
}

fn default_redis_max() -> usize { 10 }

#[derive(Debug, Deserialize, Clone)]
pub struct KafkaConfig {
    pub bootstrap_servers: String,
    pub consumer_group_id: String,
    pub topic_payments: String,
    pub topic_merchants: String,
    pub topic_webhooks: String,
    pub topic_ledger: String,
    pub topic_fraud: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TelemetryConfig {
    pub otlp_endpoint: String,
    pub service_name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}
