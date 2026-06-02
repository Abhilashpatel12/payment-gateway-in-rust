use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use common::errors::AppError;
use redis::AsyncCommands;
use uuid::Uuid;

use crate::state::GatewayState;

const IDEMPOTENCY_HEADER: &str = "Idempotency-Key";
const IDEMPOTENCY_PREFIX: &str = "idem:";




pub async fn idempotency_middleware(
    State(state): State<GatewayState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let idempotency_key = req
        .headers()
        .get(IDEMPOTENCY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    
    let Some(key) = idempotency_key else {
        return Ok(next.run(req).await);
    };

    if key.len() > 255 {
        return Err(AppError::InvalidIdempotencyKey(
            "Idempotency key too long (max 255 chars)".into(),
        ));
    }

    
    if !matches!(req.method(), &axum::http::Method::POST | &axum::http::Method::PUT | &axum::http::Method::PATCH) {
        return Ok(next.run(req).await);
    }

    
    let merchant_id = req.extensions().get::<Uuid>().copied();
    let Some(merchant_id) = merchant_id else {
        return Ok(next.run(req).await);
    };

    let redis_key = format!("{IDEMPOTENCY_PREFIX}{merchant_id}:{key}");

    
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| AppError::Cache(e.to_string()))?;

    let cached: Option<String> = conn
        .get(&redis_key)
        .await
        .map_err(|e| AppError::Cache(e.to_string()))?;

    if let Some(cached_response) = cached {
        tracing::info!(key = %key, "Returning idempotent cached response");
        
        let body = axum::body::Body::from(cached_response);
        return Ok(axum::response::Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .header("X-Idempotent-Replayed", "true")
            .body(body)
            .unwrap());
    }

    
    let _: bool = conn
        .set_ex(&redis_key, "in-flight", 30)
        .await
        .map_err(|e| AppError::Cache(e.to_string()))?;

    
    let response = next.run(req).await;
    let status = response.status();

    
    if status.is_success() {
        let ttl = state.config.idempotency_ttl_seconds;
        
        
        let _: () = conn
            .set_ex(&redis_key, "completed", ttl)
            .await
            .unwrap_or(());
    } else {
        
        let _: () = conn.del(&redis_key).await.unwrap_or(());
    }

    Ok(response)
}
