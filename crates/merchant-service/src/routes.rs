use axum::{routing::{get, post}, Router};
use crate::{handlers, state::MerchantState};

pub fn build_routes(state: MerchantState) -> Router {
    Router::new()
        .route("/merchants", post(handlers::create_merchant))
        .route("/merchants/:id", get(handlers::get_merchant).patch(handlers::update_merchant))
        .route("/merchants/:id/rotate-key", post(handlers::rotate_api_key))
        .route("/merchants/:id/balance", get(handlers::get_balance))
        .route("/merchants/:id/webhooks", post(handlers::register_webhook))
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::readiness))
        .with_state(state)
}
