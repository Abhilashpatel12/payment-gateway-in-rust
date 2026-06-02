use axum::{
    routing::{get, post},
    Router,
};

use crate::{handlers, state::AppState};

pub fn build_routes(state: AppState) -> Router {
    let auth_routes = Router::new()
        .route("/payments", post(handlers::create_payment).get(handlers::list_payments))
        .route("/payments/:id", get(handlers::get_payment))
        .route("/payments/:id/capture", post(handlers::capture_payment))
        .route("/payments/:id/refund", post(handlers::create_refund))
        .route("/payments/:id/refunds", get(handlers::list_refunds))
        .route("/payments/:id/cancel", post(handlers::cancel_payment))
        .layer(axum::middleware::from_fn(extract_merchant_middleware));

    Router::new()
        .merge(auth_routes)
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::readiness))
        .with_state(state)
}

async fn extract_merchant_middleware(
    mut req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, common::errors::AppError> {
    let merchant_id_str = req
        .headers()
        .get("x-merchant-id")
        .and_then(|v| v.to_str().ok())
        .ok_or(common::errors::AppError::Unauthorized)?;

    let merchant_id = uuid::Uuid::parse_str(merchant_id_str)
        .map_err(|_| common::errors::AppError::Unauthorized)?;

    req.extensions_mut().insert(merchant_id);

    Ok(next.run(req).await)
}
