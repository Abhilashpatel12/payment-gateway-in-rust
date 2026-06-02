use ::metrics::{counter, histogram};
use std::time::Duration;

pub fn record_payment_created(currency: &str) {
    counter!("payments_created_total", "currency" => currency.to_string()).increment(1);
}

pub fn record_payment_captured(currency: &str, _amount: i64) {
    counter!("payments_captured_total", "currency" => currency.to_string()).increment(1);
    
    
    
}

pub fn record_payment_failed(reason: &str) {
    counter!("payments_failed_total", "reason" => reason.to_string()).increment(1);
}

pub fn record_outbox_event_published(topic: &str) {
    counter!("outbox_events_published_total", "topic" => topic.to_string()).increment(1);
}

pub fn record_outbox_processing_duration(duration: Duration) {
    histogram!("outbox_processing_duration_seconds").record(duration.as_secs_f64());
}

pub fn record_webhook_delivery_attempt(status: &str) {
    counter!("webhook_deliveries_total", "status" => status.to_string()).increment(1);
}

pub fn record_webhook_failure() {
    counter!("webhook_deliveries_total", "status" => "failure").increment(1);
}

pub fn record_api_request_duration(endpoint: &str, method: &str, duration: Duration) {
    histogram!("http_request_duration_seconds", "endpoint" => endpoint.to_string(), "method" => method.to_string())
        .record(duration.as_secs_f64());
}

pub fn record_db_pool_wait_duration(service: &str, duration: Duration) {
    histogram!("db_pool_wait_duration_seconds", "service" => service.to_string())
        .record(duration.as_secs_f64());
}

pub fn record_db_query_duration(query_name: &str, duration: Duration) {
    histogram!("db_query_duration_seconds", "query" => query_name.to_string())
        .record(duration.as_secs_f64());
}

pub fn record_kafka_publish_latency(topic: &str, duration: Duration) {
    histogram!("kafka_publish_latency_seconds", "topic" => topic.to_string())
        .record(duration.as_secs_f64());
}

pub fn record_kafka_publish_success(topic: &str) {
    counter!("kafka_publish_success_total", "topic" => topic.to_string()).increment(1);
}

pub fn record_kafka_publish_failure(topic: &str) {
    counter!("kafka_publish_failure_total", "topic" => topic.to_string()).increment(1);
}

pub fn record_payment_request_duration(endpoint: &str, duration: Duration) {
    histogram!("payment_request_duration_seconds", "endpoint" => endpoint.to_string())
        .record(duration.as_secs_f64());
}

pub fn spawn_telemetry_loop(pool: sqlx::PgPool, service_name: &'static str) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;

            
            let handle = tokio::runtime::Handle::current();
            let metrics = handle.metrics();
            ::metrics::gauge!("tokio_active_tasks", "service" => service_name).set(metrics.num_alive_tasks() as f64);

            
            ::metrics::gauge!("db_pool_size", "service" => service_name).set(pool.size() as f64);
            ::metrics::gauge!("db_pool_idle", "service" => service_name).set(pool.num_idle() as f64);
        }
    });
}
