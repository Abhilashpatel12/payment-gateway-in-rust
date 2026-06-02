#![allow(dead_code, unused_variables, unused_imports)]

mod config;
mod db;
mod handlers;
mod routes;
mod state;

use anyhow::Context;
use axum::Router;
use common::telemetry;
use std::sync::Arc;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    telemetry::init_telemetry("merchant-service", "http://localhost:4317").ok();
    let cfg = config::MerchantConfig::from_env()?;

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await?;

    let state = state::AppState {
        db,
        config: Arc::new(cfg),
    };

    let app = Router::new()
        .nest("/v1", routes::build_routes(state))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)));

    let addr = format!(
        "{}:{}",
        std::env::var("MERCHANT_SERVICE_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
        std::env::var("MERCHANT_SERVICE_PORT").unwrap_or_else(|_| "8082".into()),
    );

    tracing::info!(%addr, "Merchant service starting");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    telemetry::shutdown_telemetry();
    Ok(())
}
