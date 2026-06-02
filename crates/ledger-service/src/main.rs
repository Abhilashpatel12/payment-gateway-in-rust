

use anyhow::Result;
use common::{
    crypto::ledger_entry_hash,
    models::EntryType,
};
use futures::StreamExt;
use rdkafka::{consumer::{CommitMode, Consumer, StreamConsumer}, ClientConfig, Message};
use sqlx::PgPool;
use uuid::Uuid;
use chrono::Utc;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    common::telemetry::init_telemetry("ledger-service", "http://localhost:4317").ok();

    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&std::env::var("DATABASE_URL")?)
        .await?;

    let bootstrap_servers = std::env::var("KAFKA_BOOTSTRAP_SERVERS")
        .unwrap_or_else(|_| "localhost:9092".into());
    let topic = std::env::var("KAFKA_TOPIC_PAYMENTS")
        .unwrap_or_else(|_| "rustpay.payments".into());

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", &bootstrap_servers)
        .set("group.id", "rustpay-ledger-service")
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "false")
        .create()?;

    consumer.subscribe(&[&topic])?;
    tracing::info!("Ledger service consuming from {}", topic);

    let mut stream = consumer.stream();

    while let Some(message) = stream.next().await {
        if let Ok(msg) = message {
            let payload = msg.payload().unwrap_or_default();
            if let Ok(event) = serde_json::from_slice::<serde_json::Value>(payload) {
                if let Err(e) = process_event(&db, &event).await {
                    tracing::error!(error = %e, "Failed to process ledger event");
                }
            }
            consumer.commit_message(&msg, CommitMode::Async)?;
        }
    }

    Ok(())
}

async fn process_event(db: &PgPool, event: &serde_json::Value) -> Result<()> {
    let event_type = event["event_type"].as_str().unwrap_or("");
    
    
    let entry_type = match event_type {
        "payment.captured" => EntryType::Credit,
        "payment.refunded" => EntryType::Debit,
        _ => return Ok(()),
    };

    let payment_id: Uuid = event["payment_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("Invalid payment_id"))?;

    let merchant_id: Uuid = event["merchant_id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| anyhow::anyhow!("Invalid merchant_id"))?;

    let amount = event["amount"].as_i64().unwrap_or(0);

    
    let balance: i64 = sqlx::query_scalar(
        "SELECT available FROM merchant_balances WHERE merchant_id = $1",
    )
    .bind(merchant_id)
    .fetch_optional(db)
    .await?
    .unwrap_or(0);

    let (balance_before, balance_after) = match entry_type {
        EntryType::Credit => (balance, balance + amount),
        EntryType::Debit => (balance, balance - amount),
    };

    
    let prev_hash: String = sqlx::query_scalar(
        "SELECT hash FROM ledger_entries WHERE merchant_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(merchant_id)
    .fetch_optional(db)
    .await?
    .unwrap_or_else(|| "genesis".to_string());

    let entry_id = Uuid::new_v4();
    let now = Utc::now();
    let hash = ledger_entry_hash(&prev_hash, &entry_id.to_string(), amount, now.timestamp());

    
    sqlx::query(
        r#"
        INSERT INTO ledger_entries
            (id, payment_id, merchant_id, entry_type, amount, currency,
             balance_before, balance_after, description, hash, created_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(entry_id)
    .bind(payment_id)
    .bind(merchant_id)
    .bind(entry_type)
    .bind(amount)
    .bind("INR")
    .bind(balance_before)
    .bind(balance_after)
    .bind(format!("Payment {} {}", event_type, payment_id))
    .bind(hash)
    .bind(now)
    .execute(db)
    .await?;

    
    sqlx::query(
        r#"
        INSERT INTO merchant_balances (merchant_id, available)
        VALUES ($1, $2)
        ON CONFLICT (merchant_id) DO UPDATE
            SET available = $2, updated_at = NOW()
        "#,
    )
    .bind(merchant_id)
    .bind(balance_after)
    .execute(db)
    .await?;

    tracing::info!(
        payment_id = %payment_id,
        entry_type = ?entry_type,
        amount = amount,
        balance_after = balance_after,
        "Ledger entry recorded"
    );

    Ok(())
}
