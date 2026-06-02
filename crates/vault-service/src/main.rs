mod handlers;
mod routes;
mod tokenizer;

use anyhow::Context;
use axum::Router;
use common::telemetry;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};




#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let service_name = std::env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| "vault-service".into());
    let otlp = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:4317".into());

    telemetry::init_telemetry(&service_name, &otlp).ok();

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .connect(&db_url)
        .await
        .context("Failed to connect to database")?;

    let master_key = std::env::var("VAULT_MASTER_KEY")
        .context("VAULT_MASTER_KEY required (32 bytes hex-encoded)")?;
    let hmac_key = std::env::var("VAULT_HMAC_KEY")
        .context("VAULT_HMAC_KEY required")?;

    let state = routes::VaultState {
        db,
        master_key,
        hmac_key,
    };

    let app = Router::new()
        .nest("/v1/vault", routes::build_routes(state))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(std::time::Duration::from_secs(10)));

    let host = std::env::var("VAULT_SERVICE_HOST").unwrap_or_else(|_| "0.0.0.0".into());
    let port = std::env::var("VAULT_SERVICE_PORT").unwrap_or_else(|_| "8085".into());
    let addr = format!("{host}:{port}");

    tracing::info!(%addr, "Vault service starting (PCI scope)");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    telemetry::shutdown_telemetry();
    Ok(())
}
