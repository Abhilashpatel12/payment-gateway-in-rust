use crate::{
    errors::{AppError, AppResult},
    models::Currency,
};
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::str::FromStr;

pub struct CurrencyConverter {
    db: PgPool,
}

impl CurrencyConverter {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    
    
    pub async fn convert(
        &self,
        amount: i64,
        from: Currency,
        to: Currency,
    ) -> AppResult<i64> {
        if from == to {
            return Ok(amount);
        }

        let rate_row = sqlx::query_scalar::<_, rust_decimal::Decimal>(
            r#"
            SELECT rate
            FROM exchange_rates
            WHERE base_currency = $1::currency_code AND target_currency = $2::currency_code
            "#,
        )
        .bind(from)
        .bind(to)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to fetch exchange rate: {}", e)))?;

        let rate = match rate_row {
            Some(rate) => rate,
            None => {
                return Err(AppError::Internal(format!(
                    "No exchange rate found for {} to {}",
                    from, to
                )));
            }
        };

        
        let rate_f64: f64 = Decimal::from_str(&rate.to_string())
            .unwrap_or(Decimal::from(1))
            .try_into()
            .unwrap_or(1.0);

        
        
        let from_dec = from.decimal_places() as i32;
        let to_dec = to.decimal_places() as i32;
        
        let mut converted = (amount as f64) * rate_f64;
        
        
        if from_dec != to_dec {
            let factor = 10_f64.powi(to_dec - from_dec);
            converted *= factor;
        }

        Ok(converted.round() as i64)
    }
}
