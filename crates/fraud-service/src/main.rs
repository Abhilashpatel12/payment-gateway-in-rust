#![allow(dead_code, unused_variables, unused_imports)]

mod handlers;
mod rules;
mod state;

use anyhow::Context;
use axum::{routing::{get, post}, Router};
use common::telemetry;
use std::sync::Arc;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    telemetry::init_telemetry("fraud-service", "http://localhost:4317").ok();

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let state = state::AppState {
        db,
        engine: Arc::new(rules::FraudEngine::default_engine()),
    };

    let app = Router::new()
        .route("/v1/risk/evaluate", post(handlers::evaluate_risk))
        .route("/health", get(handlers::health))
        .route("/ready", get(handlers::readiness))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(5)))
        .with_state(state);

    let addr = format!(
        "{}:{}",
        std::env::var("FRAUD_SERVICE_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
        std::env::var("FRAUD_SERVICE_PORT").unwrap_or_else(|_| "8086".into()),
    );

    tracing::info!(%addr, "Fraud service starting");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    telemetry::shutdown_telemetry();
    Ok(())
}
