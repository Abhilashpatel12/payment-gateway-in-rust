

use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub struct Reconciler {
    db: PgPool,
    stripe_api_key: String,
    http: reqwest::Client,
}

impl Reconciler {
    pub fn new(db: PgPool, stripe_api_key: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("HTTP client");
        Self { db, stripe_api_key, http }
    }

    
    
    pub async fn run_daily(&self) -> Result<Uuid> {
        let period_end = Utc::now();
        let period_start = period_end - Duration::hours(24);

        let run_id = Uuid::new_v4();

        
        sqlx::query(
            r#"
            INSERT INTO reconciliation_runs
                (id, provider, period_start, period_end, status, started_at)
            VALUES ($1, 'stripe', $2, $3, 'running', NOW())
            "#,
        )
        .bind(run_id)
        .bind(period_start)
        .bind(period_end)
        .execute(&self.db)
        .await?;

        tracing::info!(run_id = %run_id, "Starting daily reconciliation");

        match self.reconcile_stripe(run_id, period_start, period_end).await {
            Ok((checked, mismatches)) => {
                sqlx::query(
                    r#"
                    UPDATE reconciliation_runs
                    SET status = 'completed',
                        payments_checked = $2,
                        mismatches_found = $3,
                        completed_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(run_id)
                .bind(checked)
                .bind(mismatches)
                .execute(&self.db)
                .await?;

                tracing::info!(
                    run_id = %run_id,
                    payments_checked = checked,
                    mismatches = mismatches,
                    "Reconciliation completed"
                );
            }
            Err(e) => {
                sqlx::query(
                    r#"
                    UPDATE reconciliation_runs
                    SET status = 'failed', error = $2, completed_at = NOW()
                    WHERE id = $1
                    "#,
                )
                .bind(run_id)
                .bind(e.to_string())
                .execute(&self.db)
                .await?;
                return Err(e);
            }
        }

        Ok(run_id)
    }

    async fn reconcile_stripe(
        &self,
        run_id: Uuid,
        period_start: chrono::DateTime<Utc>,
        period_end: chrono::DateTime<Utc>,
    ) -> Result<(i32, i32)> {
        
        let our_payments = sqlx::query(
            r#"
            SELECT id, amount, acquirer_reference, status::TEXT AS status
            FROM payments
            WHERE acquirer_id = 'stripe'
              AND captured_at BETWEEN $1 AND $2
            "#,
        )
        .bind(period_start)
        .bind(period_end)
        .fetch_all(&self.db)
        .await?;

        let checked = our_payments.len() as i32;
        let mut mismatches = 0;

        
        let stripe_charges = self
            .fetch_stripe_charges(period_start.timestamp(), period_end.timestamp())
            .await?;

        
        let stripe_index: std::collections::HashMap<&str, &serde_json::Value> = stripe_charges
            .iter()
            .filter_map(|c| c["id"].as_str().map(|id| (id, c)))
            .collect();

        for payment in &our_payments {
            let payment_id: Uuid = payment.try_get("id")?;
            let amount: i64 = payment.try_get("amount")?;
            let status: Option<String> = payment.try_get("status")?;
            let acquirer_reference: Option<String> = payment.try_get("acquirer_reference")?;

            let acquirer_ref = match &acquirer_reference {
                Some(r) => r.as_str(),
                None => continue,
            };

            match stripe_index.get(acquirer_ref) {
                None => {
                    
                    self.record_mismatch(
                        run_id,
                        payment_id,
                        "missing_in_provider",
                        status.as_deref().unwrap_or("unknown"),
                        "not_found_in_stripe",
                    )
                    .await?;
                    mismatches += 1;
                }
                Some(charge) => {
                    
                    let stripe_amount = charge["amount"].as_i64().unwrap_or(0);
                    if stripe_amount != amount {
                        self.record_mismatch(
                            run_id,
                            payment_id,
                            "amount_mismatch",
                            &amount.to_string(),
                            &stripe_amount.to_string(),
                        )
                        .await?;
                        mismatches += 1;
                    }

                    
                    let stripe_status = charge["status"].as_str().unwrap_or("unknown");
                    let our_status = status.as_deref().unwrap_or("unknown");
                    
                    let expected_stripe = match our_status {
                        "captured" | "settled" => "succeeded",
                        "failed" => "failed",
                        _ => "",
                    };
                    if !expected_stripe.is_empty() && stripe_status != expected_stripe {
                        self.record_mismatch(
                            run_id,
                            payment_id,
                            "status_mismatch",
                            our_status,
                            stripe_status,
                        )
                        .await?;
                        mismatches += 1;
                    }
                }
            }
        }

        Ok((checked, mismatches))
    }

    async fn fetch_stripe_charges(
        &self,
        created_gte: i64,
        created_lte: i64,
    ) -> Result<Vec<serde_json::Value>> {
        if self.stripe_api_key.is_empty() {
            tracing::warn!("STRIPE_API_KEY not set — skipping Stripe fetch");
            return Ok(vec![]);
        }

        let resp = self
            .http
            .get("https://api.stripe.com/v1/charges")
            .basic_auth(&self.stripe_api_key, None::<&str>)
            .query(&[
                ("created[gte]", created_gte.to_string()),
                ("created[lte]", created_lte.to_string()),
                ("limit", "100".to_string()),
            ])
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        Ok(resp["data"].as_array().cloned().unwrap_or_default())
    }

    async fn record_mismatch(
        &self,
        run_id: Uuid,
        payment_id: Uuid,
        mismatch_type: &str,
        our_value: &str,
        provider_value: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO reconciliation_mismatches
                (run_id, payment_id, mismatch_type, our_value, provider_value)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(run_id)
        .bind(payment_id)
        .bind(mismatch_type)
        .bind(our_value)
        .bind(provider_value)
        .execute(&self.db)
        .await?;

        tracing::warn!(
            run_id = %run_id,
            payment_id = %payment_id,
            mismatch_type,
            our_value,
            provider_value,
            "Reconciliation mismatch found"
        );

        Ok(())
    }
}
