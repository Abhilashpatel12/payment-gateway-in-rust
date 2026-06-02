use chrono::Utc;
use common::crypto::build_webhook_signature;
use reqwest::Client;
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

pub struct WebhookDispatcher {
    db: PgPool,
    client: Client,
}

impl WebhookDispatcher {
    pub fn new(db: PgPool) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("RustPay-Webhook/1.0")
            .build()
            .expect("Failed to build HTTP client");
        Self { db, client }
    }

    
    
    pub async fn persist_and_dispatch(&self, event: serde_json::Value) -> anyhow::Result<()> {
        let merchant_id_str = event["merchant_id"].as_str().unwrap_or("");
        let merchant_id: Uuid = merchant_id_str.parse()?;
        let event_type = event["event_type"].as_str().unwrap_or("").to_string();

        
        let endpoints = sqlx::query(
            r#"
            SELECT id, url, secret_enc
            FROM webhook_endpoints
            WHERE merchant_id = $1
              AND is_active = true
              AND $2 = ANY(events)
            "#,
        )
        .bind(merchant_id)
        .bind(&event_type)
        .fetch_all(&self.db)
        .await?;

        if endpoints.is_empty() {
            return Ok(());
        }

        let payload_str = serde_json::to_string(&event)?;
        let timestamp = Utc::now().timestamp();

        for endpoint in endpoints {
            let endpoint_id: Uuid = endpoint.try_get("id")?;
            let endpoint_url: String = endpoint.try_get("url")?;
            let secret_enc: String = endpoint.try_get("secret_enc")?;

            
            
            let delivery_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO webhook_deliveries
                    (id, endpoint_id, merchant_id, event_type, payload,
                     status, attempt_count, max_attempts, next_retry_at)
                VALUES ($1, $2, $3, $4, $5, 'pending', 0, 10, NOW())
                "#,
            )
            .bind(delivery_id)
            .bind(endpoint_id)
            .bind(merchant_id)
            .bind(&event_type)
            .bind(payload_str.parse::<serde_json::Value>().unwrap_or_default())
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }
}
