









use anyhow::Result;
use chrono::Utc;
use metrics::{counter, gauge};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

const MAX_FAILED_ATTEMPTS: i32 = 10;

pub struct OutboxWorker {
    db: PgPool,
    producer: FutureProducer,
    batch_size: i64,
}

impl OutboxWorker {
    pub fn new(db: PgPool, bootstrap_servers: &str, batch_size: i64) -> Self {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("message.timeout.ms", "10000")
            .set("acks", "all")
            .set("enable.idempotence", "true")
            .set("max.in.flight.requests.per.connection", "5")
            .set("retries", "3")
            .create()
            .expect("Failed to create Kafka producer");

        Self { db, producer, batch_size }
    }

    
    pub async fn run_once(&self) -> Result<usize> {
        
        sqlx::query(
            r#"
            UPDATE outbox_events
            SET locked_at = NULL
            WHERE published = false
              AND locked_at IS NOT NULL
              AND locked_at < NOW() - INTERVAL '5 minutes'
            "#,
        )
        .execute(&self.db)
        .await?;

        
        let events = sqlx::query(
            r#"
            UPDATE outbox_events
            SET locked_at = NOW()
            WHERE id IN (
                SELECT id
                FROM outbox_events
                WHERE published = false
                  AND locked_at IS NULL
                  AND failed_attempts < $1
                ORDER BY created_at ASC
                LIMIT $2
                FOR UPDATE SKIP LOCKED
            )
            RETURNING id, aggregate_id, event_type, payload, topic, failed_attempts, created_at
            "#,
        )
        .bind(MAX_FAILED_ATTEMPTS)
        .bind(self.batch_size)
        .fetch_all(&self.db)
        .await?;

        if events.is_empty() {
            return Ok(0);
        }

        if let Some(oldest) = events.first() {
            let created_at: chrono::DateTime<Utc> = oldest.try_get("created_at")?;
            let lag_secs = (Utc::now() - created_at).num_seconds() as f64;
            gauge!("outbox_lag_seconds").set(lag_secs);
        }

        let mut futures = Vec::new();

        for event in &events {
            let event_id: Uuid = event.try_get("id")?;
            let aggregate_id: Uuid = event.try_get("aggregate_id")?;
            let event_type: String = event.try_get("event_type")?;
            let event_payload: serde_json::Value = event.try_get("payload")?;
            let topic: String = event.try_get("topic")?;
            let failed_attempts: i32 = event.try_get("failed_attempts")?;

            let payload = serde_json::to_string(&event_payload)?;
            let key = aggregate_id.to_string();
            let producer = self.producer.clone();

            let fut = async move {
                ::metrics::gauge!("active_kafka_publishes").increment(1.0);
                let start = std::time::Instant::now();
                
                let res = {
                    let record = FutureRecord::to(&topic).key(&key).payload(&payload);
                    producer.send(record, Duration::from_secs(10)).await
                };
                ::metrics::gauge!("active_kafka_publishes").decrement(1.0);
                common::metrics::record_kafka_publish_latency(&topic, start.elapsed());
                (event_id, event_type, topic, failed_attempts, res)
            };
            futures.push(fut);
        }

        use futures::stream::{self, StreamExt};
        let mut stream = stream::iter(futures).buffer_unordered(20);

        let mut published_ids: Vec<Uuid> = Vec::new();
        let mut failed: Vec<(Uuid, String)> = Vec::new();

        while let Some((event_id, event_type, topic, failed_attempts, res)) = stream.next().await {
            match res {
                Ok((partition, offset)) => {
                    tracing::debug!(
                        event_id = %event_id,
                        event_type = %event_type,
                        topic = %topic,
                        partition,
                        offset,
                        "Outbox event published"
                    );
                    published_ids.push(event_id);
                    common::metrics::record_kafka_publish_success(&topic);
                    counter!("outbox_events_published_total",
                        "topic" => topic,
                        "event_type" => event_type
                    ).increment(1);
                }
                Err((e, _)) => {
                    tracing::error!(
                        event_id = %event_id,
                        error = %e,
                        attempts = failed_attempts + 1,
                        "Failed to publish outbox event"
                    );
                    failed.push((event_id, e.to_string()));
                    common::metrics::record_kafka_publish_failure(&topic);
                    counter!("outbox_events_failed_total").increment(1);
                }
            }
        }

        let mut tx = self.db.begin().await?;

        if !published_ids.is_empty() {
            sqlx::query(
                r#"
                UPDATE outbox_events
                SET published = true, published_at = NOW(), locked_at = NULL
                WHERE id = ANY($1)
                "#,
            )
            .bind(&published_ids)
            .execute(&mut *tx)
            .await?;
        }

        for (id, error) in &failed {
            sqlx::query(
                r#"
                UPDATE outbox_events
                SET failed_attempts = failed_attempts + 1,
                    last_error = $2,
                    locked_at = NULL
                WHERE id = $1
                "#,
            )
            .bind(id)
            .bind(error)
            .execute(&mut *tx)
            .await?;
        }

        for dead in &events {
            let id: Uuid = dead.try_get("id")?;
            let failed_attempts: i32 = dead.try_get("failed_attempts")?;
            if !failed.iter().any(|(failed_id, _)| *failed_id == id)
                || failed_attempts + 1 < MAX_FAILED_ATTEMPTS
            {
                continue;
            }
            let event_type: String = dead.try_get("event_type")?;
            let aggregate_id: Uuid = dead.try_get("aggregate_id")?;
            tracing::error!(
                event_id = %id,
                event_type = %event_type,
                aggregate_id = %aggregate_id,
                "Outbox event dead-lettered after {} failed attempts",
                MAX_FAILED_ATTEMPTS
            );
            counter!("outbox_events_dead_lettered_total").increment(1);
        }

        tx.commit().await?;
        Ok(published_ids.len())
    }
}
