use axum::{
    extract::{Request, State},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use base64::Engine;
use common::{
    crypto::hash_api_key,
    errors::AppError,
};
use sqlx::Row;
use sqlx::PgPool;
use uuid::Uuid;

use crate::state::GatewayState;


#[derive(Debug, Clone)]
pub struct AuthenticatedMerchant {
    pub id: Uuid,
    pub business_name: String,
    pub is_live: bool,
}




pub async fn auth_middleware(
    State(state): State<GatewayState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let headers = req.headers();
    let api_key = extract_api_key(headers)?;

    
    let is_live = api_key.starts_with("rp_live_");
    let is_test = api_key.starts_with("rp_test_");

    if !is_live && !is_test {
        return Err(AppError::Unauthorized);
    }

    
    let key_hash = hash_api_key(&api_key);

    let merchant = lookup_merchant_by_key_hash(&state.db, &key_hash, is_live)
        .await?
        .ok_or(AppError::Unauthorized)?;

    tracing::info!(
        merchant_id = %merchant.id,
        is_live = merchant.is_live,
        "Authenticated merchant"
    );

    
    req.extensions_mut().insert(merchant.id);
    req.extensions_mut().insert(merchant);

    Ok(next.run(req).await)
}

fn extract_api_key(headers: &HeaderMap) -> Result<String, AppError> {
    
    if let Some(val) = headers.get("X-Api-Key") {
        let key = val.to_str().map_err(|_| AppError::Unauthorized)?;
        return Ok(key.to_string());
    }

    
    if let Some(auth) = headers.get("Authorization") {
        let auth_str = auth.to_str().map_err(|_| AppError::Unauthorized)?;
        if let Some(encoded) = auth_str.strip_prefix("Basic ") {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| AppError::Unauthorized)?;
            let credentials = String::from_utf8(decoded).map_err(|_| AppError::Unauthorized)?;
            
            let key = credentials.split(':').next().unwrap_or("").to_string();
            if !key.is_empty() {
                return Ok(key);
            }
        } else if let Some(key) = auth_str.strip_prefix("Bearer ") {
            if !key.is_empty() {
                return Ok(key.to_string());
            }
        }
    }

    Err(AppError::Unauthorized)
}

async fn lookup_merchant_by_key_hash(
    db: &PgPool,
    key_hash: &str,
    is_live: bool,
) -> Result<Option<AuthenticatedMerchant>, AppError> {
    let row = if is_live {
        sqlx::query(
            r#"
            SELECT id, business_name
            FROM merchants
            WHERE api_key_hash = $1 AND is_active = true
            LIMIT 1
            "#,
        )
        .bind(key_hash)
        .fetch_optional(db)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT id, business_name
            FROM merchants
            WHERE test_api_key_hash = $1 AND is_active = true
            LIMIT 1
            "#,
        )
        .bind(key_hash)
        .fetch_optional(db)
        .await?
    };

    Ok(row.map(|r| AuthenticatedMerchant {
        id: r.get("id"),
        business_name: r.get("business_name"),
        is_live,
    }))
}
