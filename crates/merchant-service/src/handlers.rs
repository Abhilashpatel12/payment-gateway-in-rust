use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use common::{
    errors::{AppError, AppResult},
    models::ApiResponse,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::{db, state::MerchantState};



#[derive(Debug, Deserialize, Validate)]
pub struct CreateMerchantRequest {
    #[validate(length(min = 1, max = 200))]
    pub business_name: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(max = 20))]
    pub phone: Option<String>,
    #[validate(url)]
    pub website: Option<String>,
}

pub async fn create_merchant(
    State(state): State<MerchantState>,
    Json(req): Json<CreateMerchantRequest>,
) -> AppResult<impl IntoResponse> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    let result = db::create_merchant(
        &state.db,
        db::CreateMerchantInput {
            business_name: req.business_name,
            email: req.email,
            phone: req.phone,
            website: req.website,
        },
        &std::env::var("VAULT_MASTER_KEY").unwrap_or_else(|_| {
            "0000000000000000000000000000000000000000000000000000000000000000".into()
        }),
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    
    #[derive(Serialize)]
    struct CreateMerchantResponse {
        merchant: common::models::Merchant,
        live_api_key: String,
        test_api_key: String,
        message: &'static str,
    }

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(
            CreateMerchantResponse {
                merchant: result.merchant,
                live_api_key: result.live_api_key,
                test_api_key: result.test_api_key,
                message: "Save these API keys — they will not be shown again.",
            },
            new_request_id(),
        )),
    ))
}



pub async fn get_merchant(
    State(state): State<MerchantState>,
    Path(merchant_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let merchant = db::get_merchant_by_id(&state.db, merchant_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Merchant {merchant_id} not found")))?;

    Ok(Json(ApiResponse::ok(merchant, new_request_id())))
}



#[derive(Debug, Deserialize)]
pub struct UpdateMerchantRequest {
    pub business_name: Option<String>,
    pub phone: Option<String>,
    pub website: Option<String>,
    pub webhook_url: Option<String>,
    
    pub version: i64,
}

pub async fn update_merchant(
    State(state): State<MerchantState>,
    Path(merchant_id): Path<Uuid>,
    Json(req): Json<UpdateMerchantRequest>,
) -> AppResult<impl IntoResponse> {
    let updated = db::update_merchant(
        &state.db,
        merchant_id,
        req.business_name,
        req.phone,
        req.website,
        req.webhook_url,
        req.version,
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    if !updated {
        return Err(AppError::Validation(
            "Optimistic locking conflict — another update was applied. Refetch and retry.".into(),
        ));
    }

    let merchant = db::get_merchant_by_id(&state.db, merchant_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or_else(|| AppError::NotFound(format!("Merchant {merchant_id}")))?;

    Ok(Json(ApiResponse::ok(merchant, new_request_id())))
}



#[derive(Debug, Deserialize)]
pub struct RotateKeyRequest {
    
    pub is_live: bool,
}

pub async fn rotate_api_key(
    State(state): State<MerchantState>,
    Path(merchant_id): Path<Uuid>,
    Json(req): Json<RotateKeyRequest>,
) -> AppResult<impl IntoResponse> {
    let new_key = db::rotate_api_key(&state.db, merchant_id, req.is_live)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    #[derive(Serialize)]
    struct RotateKeyResponse {
        new_key: String,
        is_live: bool,
        message: &'static str,
    }

    Ok(Json(ApiResponse::ok(
        RotateKeyResponse {
            new_key,
            is_live: req.is_live,
            message: "This key will not be shown again. Update your integration immediately.",
        },
        new_request_id(),
    )))
}



pub async fn get_balance(
    State(state): State<MerchantState>,
    Path(merchant_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    let balance = db::get_merchant_balance(&state.db, merchant_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    #[derive(Serialize)]
    struct BalanceResponse {
        merchant_id: Uuid,
        available: i64,
        pending: i64,
        reserved: i64,
    }

    Ok(Json(ApiResponse::ok(
        BalanceResponse {
            merchant_id,
            available: balance.available,
            pending: balance.pending,
            reserved: balance.reserved,
        },
        new_request_id(),
    )))
}



#[derive(Debug, Deserialize, Validate)]
pub struct RegisterWebhookRequest {
    #[validate(url)]
    pub url: String,
    pub events: Vec<String>,
}

pub async fn register_webhook(
    State(state): State<MerchantState>,
    Path(merchant_id): Path<Uuid>,
    Json(req): Json<RegisterWebhookRequest>,
) -> AppResult<impl IntoResponse> {
    req.validate().map_err(|e| AppError::Validation(e.to_string()))?;

    let endpoint_id = db::register_webhook_endpoint(
        &state.db,
        merchant_id,
        req.url,
        req.events,
        &std::env::var("VAULT_MASTER_KEY").unwrap_or_else(|_| {
            "0000000000000000000000000000000000000000000000000000000000000000".into()
        }),
    )
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(
            serde_json::json!({ "endpoint_id": endpoint_id }),
            new_request_id(),
        )),
    ))
}



pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "merchant-service"}))
}

pub async fn readiness(State(state): State<MerchantState>) -> impl IntoResponse {
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
