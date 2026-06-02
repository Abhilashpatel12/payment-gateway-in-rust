mod db;
mod handlers;
mod routes;
mod state;

use anyhow::Context;
use axum::Router;
use common::telemetry;
use std::time::Duration;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    telemetry::init_telemetry("order-service", "http://localhost:4317").ok();

    let db_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let db = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;

    let state = state::AppState { db: db.clone() };

    let app = Router::new()
        .nest("/v1", routes::build_routes(state.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::new(Duration::from_secs(10)));

    let addr = format!(
        "{}:{}",
        std::env::var("ORDER_SERVICE_HOST").unwrap_or_else(|_| "0.0.0.0".into()),
        std::env::var("ORDER_SERVICE_PORT").unwrap_or_else(|_| "8083".into()),
    );

    
    let expire_db = db.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            if let Err(e) = expire_orders(&expire_db).await {
                tracing::error!(error = %e, "Failed to expire orders");
            }
        }
    });

    tracing::info!(%addr, "Order service starting");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    telemetry::shutdown_telemetry();
    Ok(())
}

async fn expire_orders(db: &sqlx::PgPool) -> anyhow::Result<()> {
    let affected = sqlx::query(
        r#"
        UPDATE orders
        SET status = 'expired', updated_at = NOW()
        WHERE status = 'created' AND expires_at <= NOW()
        "#
    )
    .execute(db)
    .await?
    .rows_affected();

    if affected > 0 {
        tracing::info!(expired_count = affected, "Expired orders");
    }

    Ok(())
}
