use axum::{middleware, routing::any, Router};

use crate::{auth, middleware::idempotency, proxy, state::GatewayState};

pub fn build_routes(state: GatewayState) -> Router {
    Router::new()
        .route("/health", axum::routing::get(health))
        .route("/ready", axum::routing::get(readiness))
        .route(
            "/:service/*path",
            any(proxy::proxy_downstream)
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    idempotency::idempotency_middleware,
                ))
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    auth::auth_middleware,
                )),
        )
        .with_state(state)
}

use axum::response::IntoResponse;
use axum::Json;

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "api-gateway"}))
}

async fn readiness(axum::extract::State(state): axum::extract::State<GatewayState>) -> impl IntoResponse {
    let db_ok = sqlx::query!("SELECT 1 AS one")
        .fetch_one(&state.db)
        .await
        .is_ok();
        
    let mut redis_conn = state.redis.get().await.unwrap();
    let redis_ok = deadpool_redis::redis::cmd("PING").query_async::<_, ()>(&mut redis_conn).await.is_ok();

    if db_ok && redis_ok {
        (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({"status": "ready", "checks": {"database": "ok", "redis": "ok"}})),
        )
    } else {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "not_ready", "checks": {"database": db_ok, "redis": redis_ok}})),
        )
    }
}
