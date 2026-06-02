use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use common::{
    errors::{AppError, AppResult},
    models::{ApiResponse, CreateOrderRequest},
};
use uuid::Uuid;
use validator::Validate;

use crate::{db, state::AppState};

#[tracing::instrument(skip(state), fields(merchant_id))]
pub async fn create_order(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Json(req): Json<CreateOrderRequest>,
) -> AppResult<impl IntoResponse> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;
    tracing::Span::current().record("merchant_id", merchant_id.to_string());

    let order = db::create_order(
        &state.db,
        db::CreateOrderInput {
            merchant_id,
            amount: req.amount,
            currency: req.currency,
            description: req.description,
            customer_email: req.customer_email,
            customer_id: req.customer_id,
            metadata: req.metadata,
            expires_in_minutes: req.expires_in_minutes.unwrap_or(15),
        },
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(order, new_request_id())),
    ))
}

#[tracing::instrument(skip(state))]
pub async fn get_order(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Path(order_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let order = db::get_order_by_id(&state.db, order_id, merchant_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Order {order_id} not found")))?;

    Ok(Json(ApiResponse::ok(order, new_request_id())))
}

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "order-service"}))
}

pub async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();

    if db_ok {
        (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ready", "checks": {"database": "ok"}})),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "not_ready", "checks": {"database": "failed"}})),
        )
    }
}

fn new_request_id() -> String {
    format!("req_{}", Uuid::new_v4().to_string().replace('-', ""))
}
