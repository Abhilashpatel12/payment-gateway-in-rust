





use common::{
    errors::{AppError, AppResult},
    models::{
        CaptureMethod, Currency, PaginatedResponse, PaginationParams, Payment, PaymentStatus,
        Refund, RefundStatus,
    },
};
use sqlx::{postgres::PgRow, PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

const PAYMENT_SELECT: &str = r#"
    SELECT
        id, merchant_id, order_id, amount, currency, status,
        payment_method, description, metadata,
        acquirer_id, acquirer_reference,
        failure_code, failure_message,
        capture_method, captured_amount, refunded_amount,
        idempotency_key,
        created_at, updated_at, captured_at, settled_at, expires_at
    FROM payments
"#;

pub async fn insert_payment_idempotent(
    tx: &mut Transaction<'_, Postgres>,
    payment: &Payment,
) -> AppResult<(Uuid, bool)> {
    let payment_method_json =
        serde_json::to_value(&payment.payment_method).map_err(AppError::Serialization)?;

    let returned_id: Option<Uuid> = sqlx::query_scalar(
        r#"
        INSERT INTO payments (
            id, merchant_id, order_id, amount, currency, status,
            payment_method, description, metadata, capture_method,
            idempotency_key, created_at, updated_at, expires_at,
            captured_amount, refunded_amount
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9, $10,
            $11, $12, $13, $14,
            $15, 0
        )
        ON CONFLICT (merchant_id, idempotency_key) WHERE idempotency_key IS NOT NULL
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(payment.id)
    .bind(payment.merchant_id)
    .bind(payment.order_id)
    .bind(payment.amount)
    .bind(payment.currency)
    .bind(payment.status)
    .bind(payment_method_json)
    .bind(&payment.description)
    .bind(&payment.metadata)
    .bind(payment.capture_method)
    .bind(&payment.idempotency_key)
    .bind(payment.created_at)
    .bind(payment.updated_at)
    .bind(payment.expires_at)
    .bind(payment.captured_amount)
    .fetch_optional(&mut **tx)
    .await?;

    match returned_id {
        Some(id) => Ok((id, true)),
        None => {
            let existing_id: Uuid = sqlx::query_scalar(
                r#"
                SELECT id FROM payments
                WHERE merchant_id = $1 AND idempotency_key = $2
                "#,
            )
            .bind(payment.merchant_id)
            .bind(&payment.idempotency_key)
            .fetch_one(&mut **tx)
            .await?;
            Ok((existing_id, false))
        }
    }
}

pub async fn lock_payment_for_update(
    tx: &mut Transaction<'_, Postgres>,
    payment_id: Uuid,
    merchant_id: Uuid,
) -> AppResult<Payment> {
    let row = sqlx::query(&format!(
        "{PAYMENT_SELECT} WHERE id = $1 AND merchant_id = $2 FOR UPDATE"
    ))
    .bind(payment_id)
    .bind(merchant_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Payment {payment_id} not found")))?;

    row_to_payment(row)
}

pub async fn update_payment_after_charge(
    tx: &mut Transaction<'_, Postgres>,
    payment: &Payment,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE payments SET
            status             = $2,
            acquirer_id        = $3,
            acquirer_reference = $4,
            captured_at        = $5,
            captured_amount    = $6,
            updated_at         = NOW()
        WHERE id = $1
        "#,
    )
    .bind(payment.id)
    .bind(payment.status)
    .bind(&payment.acquirer_id)
    .bind(&payment.acquirer_reference)
    .bind(payment.captured_at)
    .bind(payment.captured_amount)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn update_payment_status(
    tx: &mut Transaction<'_, Postgres>,
    payment_id: Uuid,
    status: PaymentStatus,
    failure_code: Option<String>,
    failure_message: Option<String>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE payments SET
            status          = $2,
            failure_code    = $3,
            failure_message = $4,
            updated_at      = NOW()
        WHERE id = $1
        "#,
    )
    .bind(payment_id)
    .bind(status)
    .bind(failure_code)
    .bind(failure_message)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn increment_refunded_amount(
    tx: &mut Transaction<'_, Postgres>,
    payment_id: Uuid,
    refund_amount: i64,
) -> AppResult<()> {
    let rows_affected = sqlx::query(
        r#"
        UPDATE payments
        SET refunded_amount = refunded_amount + $2, updated_at = NOW()
        WHERE id = $1
          AND refunded_amount + $2 <= COALESCE(captured_amount, amount)
        "#,
    )
    .bind(payment_id)
    .bind(refund_amount)
    .execute(&mut **tx)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::AmountExceedsLimit {
            amount: refund_amount,
            max: 0,
        });
    }
    Ok(())
}

pub async fn mark_payment_settled(
    tx: &mut Transaction<'_, Postgres>,
    payment_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE payments
        SET status = 'settled', settled_at = NOW(), updated_at = NOW()
        WHERE id = $1 AND status = 'captured'
        "#,
    )
    .bind(payment_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_outbox_event(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    event_type: &str,
    payload: &serde_json::Value,
    topic: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO outbox_events (aggregate_type, aggregate_id, event_type, payload, topic)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(event_type)
    .bind(payload)
    .bind(topic)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_refund(tx: &mut Transaction<'_, Postgres>, refund: &Refund) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO refunds (
            id, payment_id, merchant_id, amount, currency, status,
            reason, idempotency_key, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (merchant_id, idempotency_key) WHERE idempotency_key IS NOT NULL
        DO NOTHING
        "#,
    )
    .bind(refund.id)
    .bind(refund.payment_id)
    .bind(refund.merchant_id)
    .bind(refund.amount)
    .bind(refund.currency)
    .bind(refund.status)
    .bind(&refund.reason)
    .bind(&refund.idempotency_key)
    .bind(refund.created_at)
    .bind(refund.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn update_refund_status(
    db: &PgPool,
    refund_id: Uuid,
    status: RefundStatus,
    acquirer_refund_id: Option<String>,
    failure_reason: Option<String>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE refunds SET
            status             = $2,
            acquirer_refund_id = $3,
            failure_reason     = $4,
            updated_at         = NOW()
        WHERE id = $1
        "#,
    )
    .bind(refund_id)
    .bind(status)
    .bind(acquirer_refund_id)
    .bind(failure_reason)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn find_payment_by_id(
    db: &PgPool,
    payment_id: Uuid,
    merchant_id: Uuid,
) -> AppResult<Option<Payment>> {
    let row = sqlx::query(&format!("{PAYMENT_SELECT} WHERE id = $1 AND merchant_id = $2"))
        .bind(payment_id)
        .bind(merchant_id)
        .fetch_optional(db)
        .await?;

    row.map(row_to_payment).transpose()
}

pub async fn find_by_idempotency_key(
    db: &PgPool,
    merchant_id: Uuid,
    key: &str,
) -> AppResult<Option<Payment>> {
    let id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM payments WHERE merchant_id = $1 AND idempotency_key = $2 LIMIT 1")
            .bind(merchant_id)
            .bind(key)
            .fetch_optional(db)
            .await?;

    match id {
        Some(id) => find_payment_by_id(db, id, merchant_id).await,
        None => Ok(None),
    }
}

pub async fn list_payments(
    db: &PgPool,
    merchant_id: Uuid,
    pagination: &PaginationParams,
) -> AppResult<PaginatedResponse<Payment>> {
    let per_page = pagination.per_page.min(100) as i64;
    let offset = (pagination.page.saturating_sub(1) as i64) * per_page;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM payments WHERE merchant_id = $1")
        .bind(merchant_id)
        .fetch_one(db)
        .await?;

    let rows = sqlx::query(&format!(
        "{PAYMENT_SELECT} WHERE merchant_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3"
    ))
    .bind(merchant_id)
    .bind(per_page)
    .bind(offset)
    .fetch_all(db)
    .await?;

    let total_pages = ((total as f64) / per_page as f64).ceil() as u32;

    Ok(PaginatedResponse {
        data: rows.into_iter().map(row_to_payment).collect::<AppResult<_>>()?,
        total,
        page: pagination.page,
        per_page: pagination.per_page,
        total_pages,
    })
}

pub async fn find_refunds_for_payment(
    db: &PgPool,
    payment_id: Uuid,
    merchant_id: Uuid,
) -> AppResult<Vec<Refund>> {
    let rows = sqlx::query(
        r#"
        SELECT
            id, payment_id, merchant_id, amount, currency, status,
            reason, acquirer_refund_id, failure_reason,
            idempotency_key, created_at, updated_at
        FROM refunds
        WHERE payment_id = $1 AND merchant_id = $2
        ORDER BY created_at DESC
        "#,
    )
    .bind(payment_id)
    .bind(merchant_id)
    .fetch_all(db)
    .await?;

    rows.into_iter().map(row_to_refund).collect()
}

fn row_to_payment(row: PgRow) -> AppResult<Payment> {
    let payment_method: Option<serde_json::Value> = row.try_get("payment_method")?;
    let metadata: Option<serde_json::Value> = row.try_get("metadata")?;

    Ok(Payment {
        id: row.try_get("id")?,
        merchant_id: row.try_get("merchant_id")?,
        order_id: row.try_get("order_id")?,
        amount: row.try_get("amount")?,
        currency: row.try_get::<Currency, _>("currency")?,
        status: row.try_get::<PaymentStatus, _>("status")?,
        payment_method: payment_method.and_then(|v| serde_json::from_value(v).ok()),
        description: row.try_get("description")?,
        metadata: metadata.unwrap_or_else(|| serde_json::json!({})),
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
    })
}

fn row_to_refund(row: PgRow) -> AppResult<Refund> {
    Ok(Refund {
        id: row.try_get("id")?,
        payment_id: row.try_get("payment_id")?,
        merchant_id: row.try_get("merchant_id")?,
        amount: row.try_get("amount")?,
        currency: row.try_get::<Currency, _>("currency")?,
        status: row.try_get::<RefundStatus, _>("status")?,
        reason: row.try_get("reason")?,
        acquirer_refund_id: row.try_get("acquirer_refund_id")?,
        failure_reason: row.try_get("failure_reason")?,
        idempotency_key: row.try_get("idempotency_key")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}
