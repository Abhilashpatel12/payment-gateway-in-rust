use axum::{routing::post, Router};
use sqlx::PgPool;

use crate::handlers;

#[derive(Clone)]
pub struct VaultState {
    pub db: PgPool,
    pub master_key: String,
    pub hmac_key: String,
}

pub fn build_routes(state: VaultState) -> Router {
    Router::new()
        .route("/tokens", post(handlers::tokenize_card))
        .route("/tokens/:token/detokenize", post(handlers::detokenize_card))
        .with_state(state)
}
