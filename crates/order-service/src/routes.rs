use axum::{
    routing::{get, post},
    Router,
};

use crate::{handlers, state::AppState};

pub fn build_routes(state: AppState) -> Router {
    Router::new()
        .route("/orders", post(handlers::create_order))
        .route("/orders/:id", get(handlers::get_order))
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::readiness))
        .with_state(state)
}
