use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use common::{
    crypto::{encrypt_aes256gcm, generate_api_key, generate_webhook_secret, hash_api_key},
    models::{KycStatus, Merchant},
};
use sqlx::{postgres::PgRow, PgPool, Row};
use uuid::Uuid;

pub struct CreateMerchantInput {
    pub business_name: String,
    pub email: String,
    pub phone: Option<String>,
    pub website: Option<String>,
}

pub struct CreateMerchantResult {
    pub merchant: Merchant,
    pub live_api_key: String,
    pub test_api_key: String,
}

pub async fn create_merchant(
    db: &PgPool,
    input: CreateMerchantInput,
    vault_key_hex: &str,
) -> Result<CreateMerchantResult> {
    let live_key = generate_api_key(true);
    let test_key = generate_api_key(false);
    let webhook_secret_plain = generate_webhook_secret();

    let live_hash = hash_api_key(&live_key);
    let test_hash = hash_api_key(&test_key);
    let webhook_secret_enc = encrypt_aes256gcm(vault_key_hex, webhook_secret_plain.as_bytes())
        .context("Failed to encrypt webhook secret")?;

    let id = Uuid::new_v4();
    let now = Utc::now();
    let mut tx = db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO merchants (
            id, business_name, email, phone, website,
            kyc_status, api_key_hash, test_api_key_hash,
            webhook_secret_enc, is_active, created_at, updated_at, version
        ) VALUES ($1, $2, $3, $4, $5, 'pending', $6, $7, $8, true, $9, $9, 1)
        "#,
    )
    .bind(id)
    .bind(&input.business_name)
    .bind(&input.email)
    .bind(&input.phone)
    .bind(&input.website)
    .bind(&live_hash)
    .bind(&test_hash)
    .bind(&webhook_secret_enc)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO merchant_balances (merchant_id, available, pending, reserved)
        VALUES ($1, 0, 0, 0)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    tracing::info!(merchant_id = %id, email = %input.email, "Merchant created");

    let merchant = get_merchant_by_id(db, id)
        .await?
        .context("Merchant not found after insert")?;

    Ok(CreateMerchantResult {
        merchant,
        live_api_key: live_key,
        test_api_key: test_key,
    })
}

pub async fn update_merchant(
    db: &PgPool,
    merchant_id: Uuid,
    business_name: Option<String>,
    phone: Option<String>,
    website: Option<String>,
    webhook_url: Option<String>,
    expected_version: i64,
) -> Result<bool> {
    let rows_affected = sqlx::query(
        r#"
        UPDATE merchants
        SET
            business_name = COALESCE($3, business_name),
            phone         = COALESCE($4, phone),
            website       = COALESCE($5, website),
            webhook_url   = COALESCE($6, webhook_url),
            updated_at    = NOW()
        WHERE id = $1 AND version = $2 AND is_active = true
        "#,
    )
    .bind(merchant_id)
    .bind(expected_version)
    .bind(business_name)
    .bind(phone)
    .bind(website)
    .bind(webhook_url)
    .execute(db)
    .await?
    .rows_affected();

    Ok(rows_affected > 0)
}

pub async fn rotate_api_key(db: &PgPool, merchant_id: Uuid, is_live: bool) -> Result<String> {
    let new_key = generate_api_key(is_live);
    let new_hash = hash_api_key(&new_key);
    let column = if is_live { "api_key_hash" } else { "test_api_key_hash" };

    let q = format!("UPDATE merchants SET {column} = $1, updated_at = NOW() WHERE id = $2");
    sqlx::query(&q)
        .bind(&new_hash)
        .bind(merchant_id)
        .execute(db)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO audit_log (actor_type, actor_id, event, resource_type, resource_id)
        VALUES ('system', $1, 'api_key.rotated', 'merchant', $1)
        "#,
    )
    .bind(merchant_id)
    .execute(db)
    .await?;

    tracing::info!(merchant_id = %merchant_id, is_live, "API key rotated");
    Ok(new_key)
}

pub async fn register_webhook_endpoint(
    db: &PgPool,
    merchant_id: Uuid,
    url: String,
    events: Vec<String>,
    vault_key_hex: &str,
) -> Result<Uuid> {
    let secret = generate_webhook_secret();
    let secret_enc =
        encrypt_aes256gcm(vault_key_hex, secret.as_bytes()).context("Failed to encrypt webhook secret")?;

    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO webhook_endpoints (id, merchant_id, url, secret_enc, events)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(merchant_id)
    .bind(url)
    .bind(secret_enc)
    .bind(events)
    .execute(db)
    .await?;

    Ok(id)
}

pub async fn get_merchant_by_id(db: &PgPool, id: Uuid) -> Result<Option<Merchant>> {
    let row = sqlx::query(
        r#"
        SELECT
            id, business_name, email, phone, website,
            kyc_status, api_key_hash, test_api_key_hash,
            webhook_url, webhook_secret_enc,
            fee_plan_id, is_active, created_at, updated_at
        FROM merchants WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    row.map(row_to_merchant).transpose()
}

pub async fn get_merchant_by_api_key_hash(
    db: &PgPool,
    api_key_hash: &str,
) -> Result<Option<Merchant>> {
    let row = sqlx::query(
        r#"
        SELECT
            id, business_name, email, phone, website,
            kyc_status, api_key_hash, test_api_key_hash,
            webhook_url, webhook_secret_enc,
            fee_plan_id, is_active, created_at, updated_at
        FROM merchants
        WHERE (api_key_hash = $1 OR test_api_key_hash = $1)
          AND is_active = true
        "#,
    )
    .bind(api_key_hash)
    .fetch_optional(db)
    .await?;

    row.map(row_to_merchant).transpose()
}

pub struct MerchantBalance {
    pub available: i64,
    pub pending: i64,
    pub reserved: i64,
}

pub async fn get_merchant_balance(db: &PgPool, merchant_id: Uuid) -> Result<MerchantBalance> {
    let row = sqlx::query(
        "SELECT available, pending, reserved FROM merchant_balances WHERE merchant_id = $1",
    )
    .bind(merchant_id)
    .fetch_optional(db)
    .await?;

    Ok(match row {
        Some(r) => MerchantBalance {
            available: r.try_get("available")?,
            pending: r.try_get("pending")?,
            reserved: r.try_get("reserved")?,
        },
        None => MerchantBalance {
            available: 0,
            pending: 0,
            reserved: 0,
        },
    })
}

fn row_to_merchant(row: PgRow) -> Result<Merchant> {
    Ok(Merchant {
        id: row.try_get("id")?,
        business_name: row.try_get("business_name")?,
        email: row.try_get("email")?,
        phone: row.try_get("phone")?,
        website: row.try_get("website")?,
        kyc_status: row.try_get::<KycStatus, _>("kyc_status")?,
        api_key_hash: row.try_get("api_key_hash")?,
        test_api_key_hash: row.try_get("test_api_key_hash")?,
        webhook_url: row.try_get("webhook_url")?,
        webhook_secret_enc: row.try_get("webhook_secret_enc")?,
        fee_plan_id: row.try_get("fee_plan_id")?,
        is_active: row.try_get("is_active")?,
        created_at: row.try_get::<DateTime<Utc>, _>("created_at")?,
        updated_at: row.try_get::<DateTime<Utc>, _>("updated_at")?,
    })
}
