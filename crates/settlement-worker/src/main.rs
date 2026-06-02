mod settler;

use anyhow::{Context, Result};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().json().init();

    let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL required")?;
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    let system_fee_percentage = std::env::var("SYSTEM_FEE_PERCENTAGE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.02);
    let settler = settler::Settler::new(db, system_fee_percentage);

    tracing::info!("Settlement worker started");

    loop {
        let now = chrono::Utc::now();
        
        let next_midnight = (now.date_naive() + chrono::Duration::days(1))
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let secs_until = (next_midnight - now).num_seconds().max(0) as u64;

        tracing::info!(secs_until_next_run = secs_until, "Waiting for next settlement window");
        tokio::time::sleep(Duration::from_secs(secs_until)).await;

        match settler.run_settlement_batch().await {
            Ok(settled) => tracing::info!(merchants_settled = settled, "Settlement batch complete"),
            Err(e) => tracing::error!(error = %e, "Settlement batch failed"),
        }
    }
}
