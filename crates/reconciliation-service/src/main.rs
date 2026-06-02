mod reconciler;

use anyhow::{Context, Result};
use axum::{routing::post, Json, Router};
use sqlx::postgres::PgPoolOptions;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().json().init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let stripe_api_key = std::env::var("STRIPE_API_KEY").unwrap_or_default();

    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    let reconciler = std::sync::Arc::new(reconciler::Reconciler::new(db.clone(), stripe_api_key));

    
    let rec_clone = reconciler.clone();
    tokio::spawn(async move {
        loop {
            
            let now = chrono::Utc::now();
            let next_midnight = (now.date_naive() + chrono::Duration::days(1))
                .and_hms_opt(2, 0, 0)
                .unwrap()
                .and_utc();
            let until_next = next_midnight - now;
            let secs = until_next.num_seconds().max(0) as u64;

            tracing::info!(secs_until_next_run = secs, "Next reconciliation scheduled");
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;

            if let Err(e) = rec_clone.run_daily().await {
                tracing::error!(error = %e, "Daily reconciliation failed");
            }
        }
    });

    
    let app = Router::new()
        .route("/v1/reconcile", post({
            let rec = reconciler.clone();
            move || {
                let rec = rec.clone();
                async move {
                    match rec.run_daily().await {
                        Ok(run_id) => Json(serde_json::json!({
                            "status": "completed",
                            "run_id": run_id
                        })),
                        Err(e) => Json(serde_json::json!({
                            "status": "failed",
                            "error": e.to_string()
                        })),
                    }
                }
            }
        }))
        .route("/health", axum::routing::get(|| async {
            Json(serde_json::json!({"status": "ok", "service": "reconciliation-service"}))
        }));

    let addr: SocketAddr = "0.0.0.0:8090".parse()?;
    tracing::info!(%addr, "Reconciliation service starting");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
