use anyhow::Result;
use chrono::Utc;
use common::models::{Order, OrderStatus, Currency};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct CreateOrderInput {
    pub merchant_id: Uuid,
    pub amount: i64,
    pub currency: Currency,
    pub description: Option<String>,
    pub customer_email: Option<String>,
    pub customer_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub expires_in_minutes: i64,
}

pub async fn create_order(db: &PgPool, input: CreateOrderInput) -> Result<Order> {
    let order_id = Uuid::new_v4();
    let now = Utc::now();
    let expires_at = now + chrono::Duration::minutes(input.expires_in_minutes);

    let metadata_json = input.metadata.unwrap_or_else(|| serde_json::json!({}));

    sqlx::query(
        r#"
        INSERT INTO orders (
            id, merchant_id, amount, currency, status,
            customer_email, customer_id, description, metadata,
            expires_at, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, 'created', $5, $6, $7, $8, $9, $10, $10)
        "#,
    )
    .bind(order_id)
    .bind(input.merchant_id)
    .bind(input.amount)
    .bind(input.currency)
    .bind(&input.customer_email)
    .bind(&input.customer_id)
    .bind(&input.description)
    .bind(metadata_json)
    .bind(expires_at)
    .bind(now)
    .execute(db)
    .await?;

    get_order_by_id(db, order_id, input.merchant_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Order not found after insert"))
}

pub async fn get_order_by_id(
    db: &PgPool,
    order_id: Uuid,
    merchant_id: Uuid,
) -> Result<Option<Order>> {
    let row = sqlx::query(
        r#"
        SELECT
            id, merchant_id, amount, currency,
            status, customer_email, customer_id,
            description, metadata, payment_id,
            expires_at, created_at, updated_at
        FROM orders
        WHERE id = $1 AND merchant_id = $2
        "#,
    )
    .bind(order_id)
    .bind(merchant_id)
    .fetch_optional(db)
    .await?;

    if let Some(r) = row {
        Ok(Some(Order {
            id: r.try_get("id")?,
            merchant_id: r.try_get("merchant_id")?,
            amount: r.try_get("amount")?,
            currency: r.try_get::<Currency, _>("currency")?,
            status: r.try_get::<OrderStatus, _>("status")?,
            customer_email: r.try_get("customer_email")?,
            customer_id: r.try_get("customer_id")?,
            description: r.try_get("description")?,
            metadata: r
                .try_get::<Option<serde_json::Value>, _>("metadata")?
                .unwrap_or_else(|| serde_json::json!({})),
            payment_id: r.try_get("payment_id")?,
            expires_at: r.try_get("expires_at")?,
            created_at: r.try_get("created_at")?,
            updated_at: r.try_get("updated_at")?,
        }))
    } else {
        Ok(None)
    }
}
