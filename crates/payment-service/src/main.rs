#![allow(dead_code, unused_variables, unused_imports)]

mod config;
mod db;
mod dispute;
mod events;
mod handlers;
mod ledger;
mod metrics;
mod orchestrator;
mod routes;
mod state;
mod state_machine;

use anyhow::Context;
use axum::Router;
use common::telemetry;
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    
    dotenvy::dotenv().ok();
    let cfg = config::PaymentServiceConfig::from_env()?;

    
    telemetry::init_telemetry(&cfg.telemetry.service_name, &cfg.telemetry.otlp_endpoint)
        .context("Failed to initialize telemetry")?;

    
    let db = PgPoolOptions::new()
        .max_connections(cfg.database.max_connections)
        .min_connections(cfg.database.min_connections)
        .connect(&cfg.database.url)
        .await
        .context("Failed to connect to database")?;

    
    sqlx::migrate!("../../migrations")
        .run(&db)
        .await
        .context("Failed to run database migrations")?;

    tracing::info!("Database connected and migrations applied");

    
    let redis_cfg = deadpool_redis::Config::from_url(&cfg.redis.url);
    let redis = redis_cfg
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .context("Failed to create Redis pool")?;

    
    let app_state = state::AppState::new(db.clone(), redis, cfg.clone());

    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    builder
        .with_http_listener(([0, 0, 0, 0], 9100))
        .install()
        .context("Failed to install Prometheus exporter")?;

    common::metrics::spawn_telemetry_loop(db, "payment_service");

    
    let app = Router::new()
        .nest("/v1", routes::build_routes(app_state))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(30)))
        .layer(CorsLayer::permissive()); 

    let addr: SocketAddr = format!("{}:{}", cfg.server.host, cfg.server.port)
        .parse()
        .context("Invalid server address")?;

    tracing::info!(%addr, "Payment service starting");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    telemetry::shutdown_telemetry();
    tracing::info!("Payment service stopped gracefully");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("Received Ctrl+C, shutting down"),
        _ = terminate => tracing::info!("Received SIGTERM, shutting down"),
    }
}
