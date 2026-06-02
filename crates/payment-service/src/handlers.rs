use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use common::{
    errors::{AppError, AppResult},
    models::{
        ApiResponse, CaptureMethod, CreatePaymentRequest, CreateRefundRequest,
        PaginationParams, PaymentStatus,
    },
};
use tracing::instrument;
use uuid::Uuid;
use validator::Validate;

use crate::{db, orchestrator::PaymentOrchestrator, state::AppState};








#[instrument(skip(state, headers), fields(merchant_id))]
pub async fn create_payment(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    headers: HeaderMap,
    Json(req): Json<CreatePaymentRequest>,
) -> AppResult<impl IntoResponse> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;
    tracing::Span::current().record("merchant_id", merchant_id.to_string());

    
    let idempotency_key = headers
        .get("x-idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .or_else(|| req.idempotency_key.clone());

    let start = std::time::Instant::now();
    let orchestrator = PaymentOrchestrator::new(&state);
    let payment_res = orchestrator
        .create_payment(merchant_id, req, idempotency_key)
        .await;
    common::metrics::record_payment_request_duration("create_payment", start.elapsed());

    let payment = payment_res?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(payment, new_request_id())),
    ))
}



#[instrument(skip(state))]
pub async fn get_payment(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Path(payment_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let payment = db::find_payment_by_id(&state.db, payment_id, merchant_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Payment {payment_id} not found")))?;

    Ok(Json(ApiResponse::ok(payment, new_request_id())))
}



#[instrument(skip(state))]
pub async fn list_payments(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Query(pagination): Query<PaginationParams>,
) -> AppResult<impl IntoResponse> {
    let payments = db::list_payments(&state.db, merchant_id, &pagination).await?;
    Ok(Json(ApiResponse::ok(payments, new_request_id())))
}



#[instrument(skip(state, req))]
pub async fn capture_payment(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Path(payment_id): Path<Uuid>,
    Json(req): Json<CaptureRequest>,
) -> AppResult<impl IntoResponse> {
    
    if let Some(amount) = req.amount {
        if amount <= 0 {
            return Err(AppError::Validation(
                "capture amount must be positive".into(),
            ));
        }
    }

    
    let payment = db::find_payment_by_id(&state.db, payment_id, merchant_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Payment {payment_id} not found")))?;

    if payment.capture_method != CaptureMethod::Manual {
        return Err(AppError::Validation(
            "This payment uses automatic capture and cannot be captured manually".into(),
        ));
    }

    if payment.status != PaymentStatus::Authorized {
        return Err(AppError::InvalidStateTransition {
            from: format!("{:?}", payment.status),
            to: "Captured".into(),
        });
    }

    let start = std::time::Instant::now();
    let orchestrator = PaymentOrchestrator::new(&state);
    let captured_res = orchestrator
        .capture_payment(payment_id, merchant_id, req.amount)
        .await;
    common::metrics::record_payment_request_duration("capture_payment", start.elapsed());

    let captured = captured_res?;

    Ok(Json(ApiResponse::ok(captured, new_request_id())))
}



#[instrument(skip(state))]
pub async fn create_refund(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Path(payment_id): Path<Uuid>,
    Json(req): Json<CreateRefundRequest>,
) -> AppResult<impl IntoResponse> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    let start = std::time::Instant::now();
    let orchestrator = PaymentOrchestrator::new(&state);
    let refund_res = orchestrator
        .create_refund(payment_id, merchant_id, req)
        .await;
    common::metrics::record_payment_request_duration("create_refund", start.elapsed());

    let refund = refund_res?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(refund, new_request_id())),
    ))
}



#[instrument(skip(state))]
pub async fn list_refunds(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Path(payment_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let refunds = db::find_refunds_for_payment(&state.db, payment_id, merchant_id).await?;
    Ok(Json(ApiResponse::ok(refunds, new_request_id())))
}



#[instrument(skip(state))]
pub async fn cancel_payment(
    State(state): State<AppState>,
    Extension(merchant_id): Extension<Uuid>,
    Path(payment_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let orchestrator = PaymentOrchestrator::new(&state);
    let cancelled = orchestrator.cancel_payment(payment_id, merchant_id).await?;
    Ok(Json(ApiResponse::ok(cancelled, new_request_id())))
}




#[derive(Debug, serde::Deserialize)]
pub struct CaptureRequest {
    
    
    
    pub amount: Option<i64>,
}

fn new_request_id() -> String {
    format!("req_{}", Uuid::new_v4().to_string().replace('-', ""))
}




pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "payment-service"}))
}


pub async fn readiness(
    State(state): State<AppState>,
) -> impl IntoResponse {
    
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();

    
    let redis_ok = async {
        let mut conn = state.redis.get().await.ok()?;
        let pong: String = redis::cmd("PING")
            .query_async(&mut conn)
            .await
            .ok()?;
        Some(pong == "PONG")
    }
    .await
    .unwrap_or(false);

    if db_ok && redis_ok {
        (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ready",
                "checks": {"database": "ok", "redis": "ok"}
            })),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "not_ready",
                "checks": {
                    "database": if db_ok { "ok" } else { "failed" },
                    "redis": if redis_ok { "ok" } else { "failed" }
                }
            })),
        )
    }
}
