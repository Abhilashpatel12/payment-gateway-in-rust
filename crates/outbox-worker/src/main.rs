















mod worker;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("APP_LOG_LEVEL").unwrap_or_else(|_| "info".into()),
        )
        .json()
        .init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let kafka_servers = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
        .unwrap_or_else(|_| "localhost:9092".into());

    let poll_interval_ms: u64 = std::env::var("OUTBOX_POLL_INTERVAL_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    let batch_size: i64 = std::env::var("OUTBOX_BATCH_SIZE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    tracing::info!(
        poll_interval_ms,
        batch_size,
        "Outbox worker starting"
    );

    
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder
        .with_http_listener(([0, 0, 0, 0], 9101))
        .install()
        .context("Failed to install Prometheus exporter")?;

    common::metrics::spawn_telemetry_loop(db.clone(), "outbox_worker");

    let queue_db = db.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            if let Ok(count) = sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outbox_events WHERE published = false")
                .fetch_one(&queue_db)
                .await
            {
                ::metrics::gauge!("outbox_queue_length").set(count as f64);
            }

            if let Ok(Some(age_secs)) = sqlx::query_scalar::<_, Option<f64>>(
                "SELECT EXTRACT(EPOCH FROM (NOW() - MIN(created_at))) FROM outbox_events WHERE published = false"
            )
            .fetch_one(&queue_db)
            .await
            {
                ::metrics::gauge!("outbox_oldest_event_age_seconds").set(age_secs);
            }
        }
    });

    let worker = worker::OutboxWorker::new(db, &kafka_servers, batch_size);

    
    loop {
        match worker.run_once().await {
            Ok(published) => {
                if published > 0 {
                    tracing::info!(count = published, "Published outbox events to Kafka");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Outbox worker error");
            }
        }
        tokio::time::sleep(Duration::from_millis(poll_interval_ms)).await;
    }
}
