use anyhow::Result;
use chrono::Utc;
use rand::Rng;
use reqwest::Client;
use sqlx::{PgPool, Row};
use std::time::Duration;


const MAX_DELAY_SECONDS: f64 = 3600.0;

const BASE_DELAY_SECONDS: f64 = 1.0;

const JITTER_FRACTION: f64 = 0.2;

pub struct RetryScheduler {
    db: PgPool,
    client: Client,
}

impl RetryScheduler {
    pub fn new(db: PgPool) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("RustPay-Webhook/1.0")
            .build()
            .expect("Failed to build HTTP client");
        Self { db, client }
    }

    
    pub async fn run_once(&self) -> Result<usize> {
        
        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET status = 'pending', processing_started_at = NULL
            WHERE status = 'processing'
              AND processing_started_at IS NOT NULL
              AND processing_started_at < NOW() - INTERVAL '5 minutes'
            "#,
        )
        .execute(&self.db)
        .await?;

        
        let deliveries = sqlx::query(
            r#"
            UPDATE webhook_deliveries wd
            SET status = 'processing', processing_started_at = NOW()
            FROM (
                SELECT
                    wd_inner.id,
                    we.url,
                    we.secret_enc
                FROM webhook_deliveries wd_inner
                JOIN webhook_endpoints we ON we.id = wd_inner.endpoint_id
                WHERE wd_inner.status = 'pending'
                  AND wd_inner.next_retry_at <= NOW()
                  AND wd_inner.attempt_count < wd_inner.max_attempts
                ORDER BY wd_inner.next_retry_at ASC
                LIMIT 20
                FOR UPDATE OF wd_inner SKIP LOCKED
            ) sub
            WHERE wd.id = sub.id
            RETURNING
                wd.id,
                wd.endpoint_id,
                wd.event_type,
                wd.payload,
                wd.attempt_count,
                wd.max_attempts,
                sub.url,
                sub.secret_enc
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        if deliveries.is_empty() {
            return Ok(0);
        }

        let mut processed = 0;
        let mut futures = Vec::new();

        for delivery in deliveries {
            let delivery_id: uuid::Uuid = delivery.try_get("id")?;
            let event_type: String = delivery.try_get("event_type")?;
            let payload: serde_json::Value = delivery.try_get("payload")?;
            let attempt_count: i32 = delivery.try_get("attempt_count")?;
            let max_attempts: i32 = delivery.try_get("max_attempts")?;
            let url: String = delivery.try_get("url")?;
            let secret_enc: String = delivery.try_get("secret_enc")?;

            let fut = async move {
                ::metrics::gauge!("active_webhook_dispatches").increment(1.0);
                
                let payload_str = serde_json::to_string(&payload)?;
                let timestamp = Utc::now().timestamp();

                let signature = common::crypto::build_webhook_signature(
                    &secret_enc,
                    payload_str.as_bytes(),
                    timestamp,
                );

                
                sqlx::query(
                    r#"
                    UPDATE webhook_deliveries
                    SET attempt_count = attempt_count + 1, updated_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(delivery_id)
                .execute(&self.db)
                .await?;

                let result = self
                    .client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("X-RustPay-Signature", &signature)
                    .header("X-RustPay-Event", &event_type)
                    .header("X-RustPay-Delivery-Id", delivery_id.to_string())
                    .header("X-RustPay-Timestamp", timestamp.to_string())
                    .body(payload_str)
                    .send()
                    .await;

                let new_attempt = attempt_count + 1;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        sqlx::query(
                            r#"
                            UPDATE webhook_deliveries
                            SET status = 'success',
                                last_http_status = $2,
                                updated_at = NOW()
                            WHERE id = $1
                            "#,
                        )
                        .bind(delivery_id)
                        .bind(resp.status().as_u16() as i32)
                        .execute(&self.db)
                        .await?;

                        tracing::info!(
                            delivery_id = %delivery_id,
                            url = %url,
                            attempt = new_attempt,
                            "Webhook delivered"
                        );
                    }

                    Ok(resp) => {
                        let http_status = resp.status().as_u16() as i32;
                        let error_body = resp.text().await.unwrap_or_default();
                        self.handle_failure(
                            delivery_id,
                            max_attempts,
                            new_attempt,
                            http_status,
                            &format!("HTTP {http_status}: {}", &error_body[..error_body.len().min(500)]),
                        )
                        .await?;
                    }

                    Err(e) => {
                        self.handle_failure(
                            delivery_id,
                            max_attempts,
                            new_attempt,
                            0,
                            &e.to_string(),
                        )
                        .await?;
                    }
                }
                
                ::metrics::gauge!("active_webhook_dispatches").decrement(1.0);
                Ok::<(), anyhow::Error>(())
            };

            futures.push(fut);
        }

        use futures::stream::{self, StreamExt};
        let mut stream = stream::iter(futures).buffer_unordered(20);

        while let Some(res) = stream.next().await {
            if let Err(e) = res {
                tracing::error!("Error executing webhook dispatch task: {}", e);
            } else {
                processed += 1;
            }
        }

        Ok(processed)
    }

    async fn handle_failure(
        &self,
        delivery_id: uuid::Uuid,
        max_attempts: i32,
        attempt: i32,
        http_status: i32,
        error: &str,
    ) -> Result<()> {
        let next_retry = if attempt >= max_attempts {
            
            sqlx::query(
                r#"
                UPDATE webhook_deliveries
                SET status = 'dead_lettered',
                    last_http_status = $2,
                    last_error = $3,
                    dead_lettered_at = NOW(),
                    updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(delivery_id)
            .bind(http_status)
            .bind(error)
            .execute(&self.db)
            .await?;

            tracing::error!(
                delivery_id = %delivery_id,
                attempts = attempt,
                "Webhook dead-lettered after max attempts"
            );
            common::metrics::record_webhook_failure();
            return Ok(());
        } else {
            
            let base = BASE_DELAY_SECONDS * 2f64.powi(attempt - 1);
            let capped = base.min(MAX_DELAY_SECONDS);
            let jitter = rand::thread_rng().gen_range(
                -JITTER_FRACTION * capped..JITTER_FRACTION * capped,
            );
            let delay_secs = (capped + jitter).max(1.0);
            chrono::Duration::seconds(delay_secs as i64)
        };

        let next_retry_at = Utc::now() + next_retry;

        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET last_http_status = $2,
                last_error = $3,
                next_retry_at = $4,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(delivery_id)
        .bind(http_status)
        .bind(error)
        .bind(next_retry_at)
        .execute(&self.db)
        .await?;

        tracing::warn!(
            delivery_id = %delivery_id,
            attempt,
            next_retry_at = %next_retry_at,
            "Webhook delivery failed — scheduled retry"
        );

        Ok(())
    }
}
