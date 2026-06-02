use anyhow::Result;
use common::models::{Dispute, DisputeStatus, PaymentStatus, Payment, Currency, CaptureMethod};
use sqlx::PgPool;
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

    
    let row = sqlx::query!(
        r#"
        SELECT
            id, merchant_id, order_id, amount,
            currency as "currency: Currency",
            status as "status: PaymentStatus",
            payment_method, description, metadata,
            acquirer_id, acquirer_reference, failure_code, failure_message,
            capture_method as "capture_method: CaptureMethod",
            captured_amount, refunded_amount,
            idempotency_key, created_at, updated_at, captured_at, settled_at, expires_at
        FROM payments
        WHERE id = $1 AND merchant_id = $2
        FOR UPDATE
        "#,
        payment_id,
        merchant_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow::anyhow!("Payment not found"))?;

    let payment = Payment {
        id: row.id,
        merchant_id: row.merchant_id,
        order_id: row.order_id,
        amount: row.amount,
        currency: row.currency,
        status: row.status,
        payment_method: row.payment_method.map(|v| serde_json::from_value(v).unwrap()),
        description: row.description,
        metadata: row.metadata.unwrap_or_default(),
        acquirer_id: row.acquirer_id,
        acquirer_reference: row.acquirer_reference,
        failure_code: row.failure_code,
        failure_message: row.failure_message,
        capture_method: row.capture_method,
        captured_amount: row.captured_amount,
        refunded_amount: row.refunded_amount,
        idempotency_key: row.idempotency_key,
        created_at: row.created_at,
        updated_at: row.updated_at,
        captured_at: row.captured_at,
        settled_at: row.settled_at,
        expires_at: row.expires_at,
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

    sqlx::query!(
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
        dispute.id,
        dispute.payment_id,
        dispute.merchant_id,
        dispute.amount,
        dispute.currency.to_string() as _,
        "needs_response" as _,
        dispute.reason_code,
        dispute.reason_description,
        dispute.evidence,
        dispute.evidence_due_by,
        dispute.evidence_submitted_at,
        dispute.resolution,
        dispute.acquirer_dispute_id,
        now,
    )
    .execute(&mut *tx)
    .await?;

    
    sqlx::query!(
        r#"
        UPDATE payments SET status = 'disputed', updated_at = NOW()
        WHERE id = $1
        "#,
        payment_id
    )
    .execute(&mut *tx)
    .await?;

    
    let payload = serde_json::to_value(&dispute).unwrap();
    sqlx::query!(
        r#"
        INSERT INTO outbox_events (id, aggregate_type, aggregate_id, event_type, payload, topic)
        VALUES ($1, 'dispute', $2, 'dispute.created', $3, 'rustpay.payments')
        "#,
        Uuid::new_v4(),
        dispute.id,
        payload
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(dispute)
}
