use axum::{
    extract::{State, Json},
    http::StatusCode,
    response::IntoResponse,
};
use common::{
    errors::{AppError, AppResult},
    models::ApiResponse,
};
use uuid::Uuid;
use validator::Validate;

use crate::{rules::FraudContext, state::AppState};

#[derive(Debug, serde::Deserialize, Validate)]
pub struct EvaluateRiskRequest {
    pub payment_id: Uuid,
    pub merchant_id: Uuid,
    #[validate(range(min = 1))]
    pub amount: i64,
    pub currency: String,
    pub payment_method: Option<serde_json::Value>,
    pub customer_email: Option<String>,
    pub ip_address: Option<String>,
}

pub async fn evaluate_risk(
    State(state): State<AppState>,
    Json(req): Json<EvaluateRiskRequest>,
) -> AppResult<impl IntoResponse> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    
    let card_token = req.payment_method.as_ref().and_then(|pm| {
        if pm["type"].as_str() == Some("card") {
            pm["token"].as_str().map(|s| s.to_string())
        } else {
            None
        }
    });

    let ctx = FraudContext {
        payment_id: req.payment_id,
        merchant_id: req.merchant_id,
        amount: req.amount,
        currency: req.currency,
        card_token,
        customer_email: req.customer_email,
        ip_address: req.ip_address,
    };

    let result = state.engine.evaluate(&ctx, &state.db).await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(ApiResponse::ok(result, new_request_id())))
}

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "fraud-service"}))
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
