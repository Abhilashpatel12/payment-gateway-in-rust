use anyhow::Result;
use chrono::Utc;
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct Settler {
    db: PgPool,
    system_fee_percentage: f64,
}

impl Settler {
    pub fn new(db: PgPool, system_fee_percentage: f64) -> Self {
        Self { db, system_fee_percentage }
    }

    
    
    pub async fn run_settlement_batch(&self) -> Result<usize> {
        
        let merchants: Vec<Uuid> = sqlx::query_scalar(
            r#"
            SELECT DISTINCT merchant_id
            FROM payments
            WHERE status = 'captured' AND settled_at IS NULL
            "#,
        )
        .fetch_all(&self.db)
        .await?;

        let mut settled_count = 0;

        for merchant_id in &merchants {
            match self.settle_merchant(*merchant_id).await {
                Ok(amount) => {
                    tracing::info!(
                        merchant_id = %merchant_id,
                        net_amount = amount,
                        "Merchant settled"
                    );
                    metrics::counter!("settlements_completed_total").increment(1);
                    settled_count += 1;
                }
                Err(e) => {
                    tracing::error!(
                        merchant_id = %merchant_id,
                        error = %e,
                        "Merchant settlement failed"
                    );
                }
            }
        }

        Ok(settled_count)
    }

    async fn settle_merchant(&self, merchant_id: Uuid) -> Result<i64> {
        let mut tx = self.db.begin().await?;

        
        let payments = sqlx::query(
            r#"
            SELECT id, captured_amount, amount, currency::TEXT AS currency
            FROM payments
            WHERE merchant_id = $1
              AND status = 'captured'
              AND settled_at IS NULL
            FOR UPDATE SKIP LOCKED
            "#,
        )
        .bind(merchant_id)
        .fetch_all(&mut *tx)
        .await?;

        if payments.is_empty() {
            tx.rollback().await?;
            return Ok(0);
        }

        let total_amount: i64 = payments
            .iter()
            .map(|p| {
                let captured_amount: Option<i64> = p.try_get("captured_amount")?;
                let amount: i64 = p.try_get("amount")?;
                Ok::<i64, sqlx::Error>(captured_amount.unwrap_or(amount))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum();

        
        let fee = (total_amount as f64 * self.system_fee_percentage) as i64;
        let net_amount = total_amount - fee;

        let settlement_id = Uuid::new_v4();
        let now = Utc::now();

        
        sqlx::query(
            r#"
            INSERT INTO settlements
                (id, merchant_id, amount, currency, status, created_at)
            VALUES ($1, $2, $3, 'INR', 'completed', $4)
            "#,
        )
        .bind(settlement_id)
        .bind(merchant_id)
        .bind(net_amount)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let payment_ids: Vec<Uuid> = payments
            .iter()
            .map(|p| p.try_get("id"))
            .collect::<Result<Vec<_>, _>>()?;

        
        sqlx::query(
            r#"
            UPDATE payments
            SET status = 'settled', settled_at = $2, updated_at = $2
            WHERE id = ANY($1)
            "#,
        )
        .bind(&payment_ids)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        
        sqlx::query(
            r#"
            UPDATE merchant_balances
            SET available = available + $2,
                pending   = GREATEST(0, pending - $3)
            WHERE merchant_id = $1
            "#,
        )
        .bind(merchant_id)
        .bind(net_amount)
        .bind(total_amount)
        .execute(&mut *tx)
        .await?;

        
        sqlx::query(
            r#"
            INSERT INTO outbox_events (aggregate_type, aggregate_id, event_type, payload, topic)
            VALUES ('settlement', $1, 'settlement.completed', $2, 'rustpay.webhooks')
            "#,
        )
        .bind(settlement_id)
        .bind(serde_json::json!({
                "settlement_id": settlement_id,
                "merchant_id": merchant_id,
                "total_amount": total_amount,
                "fee_amount": fee,
                "net_amount": net_amount,
                "payment_count": payment_ids.len(),
                "timestamp": now,
            }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        tracing::info!(
            settlement_id = %settlement_id,
            merchant_id = %merchant_id,
            total_amount,
            fee,
            net_amount,
            payment_count = payment_ids.len(),
            "Settlement committed"
        );

        Ok(net_amount)
    }
}
