#![allow(dead_code, unused_variables, unused_imports)]

mod auth;
mod config;
mod middleware;
mod proxy;
mod routes;
mod state;

use anyhow::Context;
use axum::Router;
use common::telemetry;
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    let cfg = config::GatewayConfig::from_env()?;

    telemetry::init_telemetry(&cfg.telemetry.service_name, &cfg.telemetry.otlp_endpoint)
        .context("Failed to initialize telemetry")?;

    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to database")?;

    let redis_cfg = deadpool_redis::Config::from_url(&cfg.redis.url);
    let redis = redis_cfg
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .context("Failed to create Redis pool")?;

    let app_state = state::GatewayState::new(db, redis, cfg.clone());

    let app = Router::new()
        .nest("/v1", routes::build_routes(app_state.clone()))
        .layer(
            tower::ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
                .layer(axum::middleware::from_fn_with_state(
                    app_state.clone(),
                    middleware::rate_limit::rate_limit_middleware,
                ))
                .layer(CorsLayer::permissive()),
        );

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
        .parse()
        .context("Invalid server address")?;

    tracing::info!(%addr, "API Gateway starting");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    telemetry::shutdown_telemetry();
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install signal handler");
    tracing::info!("API Gateway shutting down");
}
