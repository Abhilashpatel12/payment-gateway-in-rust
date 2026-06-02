use anyhow::Result;
use common::models::{Dispute, DisputeStatus, PaymentStatus, Payment, Currency, CaptureMethod};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn create_dispute(
    db: &PgPool,
    payment_id: Uuid,
    merchant_id: Uuid,
    amount: i64,
    reason_code: String,
    acquirer_dispute_id: String,
) -> Result<Dispute> {
    let mut tx = db.begin().await?;

    let dispute_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    
    let row = sqlx::query(
        r#"
        SELECT
            id, merchant_id, order_id, amount, currency, status,
            payment_method, description, metadata,
            acquirer_id, acquirer_reference, failure_code, failure_message,
            capture_method,
            captured_amount, refunded_amount,
            idempotency_key, created_at, updated_at, captured_at, settled_at, expires_at
        FROM payments
        WHERE id = $1 AND merchant_id = $2
        FOR UPDATE
        "#,
    )
    .bind(payment_id)
    .bind(merchant_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow::anyhow!("Payment not found"))?;

    let payment = Payment {
        id: row.try_get("id")?,
        merchant_id: row.try_get("merchant_id")?,
        order_id: row.try_get("order_id")?,
        amount: row.try_get("amount")?,
        currency: row.try_get::<Currency, _>("currency")?,
        status: row.try_get::<PaymentStatus, _>("status")?,
        payment_method: row
            .try_get::<Option<serde_json::Value>, _>("payment_method")?
            .map(serde_json::from_value)
            .transpose()?,
        description: row.try_get("description")?,
        metadata: row
            .try_get::<Option<serde_json::Value>, _>("metadata")?
            .unwrap_or_default(),
        acquirer_id: row.try_get("acquirer_id")?,
        acquirer_reference: row.try_get("acquirer_reference")?,
        failure_code: row.try_get("failure_code")?,
        failure_message: row.try_get("failure_message")?,
        capture_method: row.try_get::<CaptureMethod, _>("capture_method")?,
        captured_amount: row.try_get("captured_amount")?,
        refunded_amount: row.try_get("refunded_amount")?,
        idempotency_key: row.try_get("idempotency_key")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        captured_at: row.try_get("captured_at")?,
        settled_at: row.try_get("settled_at")?,
        expires_at: row.try_get("expires_at")?,
    };

    let dispute = Dispute {
        id: dispute_id,
        payment_id,
        merchant_id,
        amount,
        currency: payment.currency,
        status: DisputeStatus::NeedsResponse,
        reason_code,
        reason_description: None,
        evidence: None,
        evidence_due_by: Some(now + chrono::Duration::days(7)),
        evidence_submitted_at: None,
        resolution: None,
        acquirer_dispute_id: Some(acquirer_dispute_id),
        created_at: now,
        updated_at: now,
    };

    sqlx::query(
        r#"
        INSERT INTO disputes (
            id, payment_id, merchant_id, amount, currency, status,
            reason_code, reason_description, evidence, evidence_due_by,
            evidence_submitted_at, resolution, acquirer_dispute_id,
            created_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, $5::currency_code, $6::dispute_status,
            $7, $8, $9, $10, $11, $12, $13, $14, $14
        )
        "#,
    )
    .bind(dispute.id)
    .bind(dispute.payment_id)
    .bind(dispute.merchant_id)
    .bind(dispute.amount)
    .bind(dispute.currency.to_string())
    .bind("needs_response")
    .bind(&dispute.reason_code)
    .bind(&dispute.reason_description)
    .bind(&dispute.evidence)
    .bind(dispute.evidence_due_by)
    .bind(dispute.evidence_submitted_at)
    .bind(&dispute.resolution)
    .bind(&dispute.acquirer_dispute_id)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    
    sqlx::query(
        r#"
        UPDATE payments SET status = 'disputed', updated_at = NOW()
        WHERE id = $1
        "#,
    )
    .bind(payment_id)
    .execute(&mut *tx)
    .await?;

    
    let payload = serde_json::to_value(&dispute).unwrap();
    sqlx::query(
        r#"
        INSERT INTO outbox_events (id, aggregate_type, aggregate_id, event_type, payload, topic)
        VALUES ($1, 'dispute', $2, 'dispute.created', $3, 'rustpay.payments')
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(dispute.id)
    .bind(payload)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(dispute)
}
