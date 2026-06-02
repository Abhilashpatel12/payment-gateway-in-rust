use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use common::{
    errors::AppResult,
    models::ApiResponse,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    routes::VaultState,
    tokenizer::{self, RawCardData},
};

#[derive(Debug, Deserialize)]
pub struct TokenizeCardRequest {
    pub pan: String,
    pub exp_month: u8,
    pub exp_year: u16,
    pub cvv: String,
    pub cardholder_name: Option<String>,
    pub merchant_id: Option<Uuid>,
}

pub async fn tokenize_card(
    State(state): State<VaultState>,
    Json(req): Json<TokenizeCardRequest>,
) -> AppResult<impl IntoResponse> {
    let token = tokenizer::tokenize(
        &state.db,
        &state.master_key,
        &state.hmac_key,
        RawCardData {
            pan: req.pan,
            exp_month: req.exp_month,
            exp_year: req.exp_year,
            cvv: req.cvv,
            cardholder_name: req.cardholder_name,
        },
        req.merchant_id,
    )
    .await?;

    Ok(Json(ApiResponse::ok(token, new_request_id())))
}

pub async fn detokenize_card(
    State(state): State<VaultState>,
    Path(token): Path<String>,
) -> AppResult<impl IntoResponse> {
    let data = tokenizer::detokenize(&state.db, &state.master_key, &token).await?;
    Ok(Json(ApiResponse::ok(data, new_request_id())))
}

fn new_request_id() -> String {
    format!("req_{}", Uuid::new_v4().to_string().replace('-', ""))
}
