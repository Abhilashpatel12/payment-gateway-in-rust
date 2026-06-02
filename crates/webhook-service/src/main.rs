#![allow(dead_code, unused_variables, unused_imports)]


mod consumer;
mod dispatcher;
mod retry_scheduler;

use anyhow::Context;
use common::telemetry;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    telemetry::init_telemetry("webhook-service", "http://localhost:4317").ok();

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await?;

    let bootstrap_servers = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
        .unwrap_or_else(|_| "localhost:9092".into());
    let topic = std::env::var("KAFKA_TOPIC_WEBHOOKS")
        .unwrap_or_else(|_| "rustpay.webhooks".into());

    tracing::info!("Webhook service starting — consuming from {}", topic);

    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder
        .with_http_listener(([0, 0, 0, 0], 9102))
        .install()
        .context("Failed to install Prometheus exporter")?;

    common::metrics::spawn_telemetry_loop(db.clone(), "webhook_service");

    let queue_db = db.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            
            if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM webhook_deliveries WHERE status = 'pending'")
                .fetch_one(&queue_db)
                .await
            {
                ::metrics::gauge!("webhook_queue_length").set(count as f64);
            }

            if let Ok(Some(age_secs)) = sqlx::query_scalar::<_, Option<f64>>(
                "SELECT EXTRACT(EPOCH FROM (NOW() - MIN(next_retry_at))) FROM webhook_deliveries WHERE status = 'pending'"
            )
            .fetch_one(&queue_db)
            .await
            {
                ::metrics::gauge!("webhook_oldest_pending_age_seconds").set(age_secs);
            }
        }
    });
    consumer::start(db, bootstrap_servers, topic).await?;

    telemetry::shutdown_telemetry();
    Ok(())
}
