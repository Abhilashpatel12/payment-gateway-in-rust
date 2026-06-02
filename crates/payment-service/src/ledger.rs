use chrono::Utc;
use common::{
    crypto::ledger_entry_hash,
    errors::{AppError, AppResult},
    models::{Currency, Payment, Refund},
};
use sqlx::Postgres;
use uuid::Uuid;


pub mod accounts {
    use uuid::Uuid;

    pub const GATEWAY_FLOAT: Uuid =
        Uuid::from_bytes([0x10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);
    pub const MERCHANT_PAYABLE: Uuid =
        Uuid::from_bytes([0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);
    pub const FEE_INCOME: Uuid =
        Uuid::from_bytes([0x30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);
    pub const REFUND_RESERVE: Uuid =
        Uuid::from_bytes([0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);
    pub const CHARGEBACK_RESERVE: Uuid =
        Uuid::from_bytes([0x50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01]);
}


pub struct LedgerWriter<'a, 'c> {
    tx: &'a mut sqlx::Transaction<'c, Postgres>,
}

impl<'a, 'c> LedgerWriter<'a, 'c> {
    pub fn new(tx: &'a mut sqlx::Transaction<'c, Postgres>) -> Self {
        Self { tx }
    }

    
    
    pub async fn record_capture(&mut self, payment: &Payment) -> AppResult<()> {
        let amount = payment.captured_amount.unwrap_or(payment.amount);
        let description = format!(
            "Capture for payment {} ({})",
            payment.id,
            payment.acquirer_reference.as_deref().unwrap_or("no-ref")
        );

        self.write_double_entry(
            payment.payment_id_or_id(),
            payment.merchant_id,
            payment.currency,
            amount,
            accounts::GATEWAY_FLOAT,    
            accounts::MERCHANT_PAYABLE, 
            &description,
            None,
        )
        .await
    }

    
    
    
    
    
    
    
    pub async fn record_refund(&mut self, refund: &Refund) -> AppResult<()> {
        let description = format!(
            "Refund {} for payment {} — {}",
            refund.id,
            refund.payment_id,
            refund.reason.as_deref().unwrap_or("no reason")
        );

        self.write_double_entry(
            refund.payment_id,
            refund.merchant_id,
            refund.currency,
            refund.amount,
            accounts::MERCHANT_PAYABLE, 
            accounts::GATEWAY_FLOAT,    
            &description,
            None,
        )
        .await
    }

    
    
    
    
    
    pub async fn record_fee(
        &mut self,
        payment: &Payment,
        fee_amount: i64,
    ) -> AppResult<()> {
        if fee_amount == 0 {
            return Ok(());
        }
        let description = format!(
            "Platform fee {fee_amount} {} for payment {}",
            payment.currency, payment.id
        );

        self.write_double_entry(
            payment.payment_id_or_id(),
            payment.merchant_id,
            payment.currency,
            fee_amount,
            accounts::MERCHANT_PAYABLE, 
            accounts::FEE_INCOME,       
            &description,
            None,
        )
        .await
    }

    
    
    
    
    
    pub async fn record_settlement(
        &mut self,
        payment_id: Uuid,
        merchant_id: Uuid,
        currency: Currency,
        amount: i64,
    ) -> AppResult<()> {
        let description = format!("Settlement for payment {payment_id}");

        self.write_double_entry(
            payment_id,
            merchant_id,
            currency,
            amount,
            accounts::MERCHANT_PAYABLE,   
            accounts::GATEWAY_FLOAT,      
            &description,
            Some("settlement"),
        )
        .await
    }

    

    
    
    #[allow(clippy::too_many_arguments)]
    async fn write_double_entry(
        &mut self,
        payment_id: Uuid,
        merchant_id: Uuid,
        currency: Currency,
        amount: i64,
        debit_account: Uuid,
        credit_account: Uuid,
        description: &str,
        notes: Option<&str>,
    ) -> AppResult<()> {
        let now = Utc::now();
        let debit_id = Uuid::new_v4();
        let credit_id = Uuid::new_v4();

        
        let prev_balance: i64 = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COALESCE(SUM(
                CASE WHEN entry_type = 'credit' THEN amount ELSE -amount END
            ), 0)::bigint
            FROM ledger_entries
            WHERE merchant_id = $1 AND account_id = $2
            "#,
        )
        .bind(merchant_id)
        .bind(credit_account)
        .fetch_one(&mut **self.tx)
        .await
        .map_err(AppError::Database)?;

        
        let prev_hash = self.get_prev_hash(merchant_id).await?;

        let debit_hash = ledger_entry_hash(
            &prev_hash,
            &debit_id.to_string(),
            amount,
            now.timestamp(),
        );
        let credit_hash = ledger_entry_hash(
            &debit_hash,
            &credit_id.to_string(),
            amount,
            now.timestamp(),
        );

        
        sqlx::query(
            r#"
            INSERT INTO ledger_entries
                (id, payment_id, merchant_id, entry_type, account_id, reference_entry_id,
                 amount, currency, balance_before, balance_after, description, notes, hash, created_at)
            VALUES ($1, $2, $3, 'debit', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(debit_id)
        .bind(payment_id)
        .bind(merchant_id)
        .bind(debit_account)
        .bind(credit_id)
        .bind(amount)
        .bind(currency)
        .bind(prev_balance)
        .bind(prev_balance)
        .bind(description)
        .bind(notes)
        .bind(debit_hash)
        .bind(now)
        .execute(&mut **self.tx)
        .await
        .map_err(AppError::Database)?;

        
        sqlx::query(
            r#"
            INSERT INTO ledger_entries
                (id, payment_id, merchant_id, entry_type, account_id, reference_entry_id,
                 amount, currency, balance_before, balance_after, description, notes, hash, created_at)
            VALUES ($1, $2, $3, 'credit', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
        )
        .bind(credit_id)
        .bind(payment_id)
        .bind(merchant_id)
        .bind(credit_account)
        .bind(debit_id)
        .bind(amount)
        .bind(currency)
        .bind(prev_balance)
        .bind(prev_balance + amount)
        .bind(description)
        .bind(notes)
        .bind(credit_hash)
        .bind(now)
        .execute(&mut **self.tx)
        .await
        .map_err(AppError::Database)?;

        tracing::info!(
            payment_id = %payment_id,
            merchant_id = %merchant_id,
            amount = amount,
            currency = %currency,
            debit_account = %debit_account,
            credit_account = %credit_account,
            "Double-entry ledger pair written"
        );

        Ok(())
    }

    async fn get_prev_hash(&mut self, merchant_id: Uuid) -> AppResult<String> {
        let hash = sqlx::query_scalar::<_, String>(
            "SELECT hash FROM ledger_entries WHERE merchant_id = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(merchant_id)
        .fetch_optional(&mut **self.tx)
        .await
        .map_err(AppError::Database)?
        .unwrap_or_else(|| "genesis".to_string());

        Ok(hash)
    }
}



trait PaymentId {
    fn payment_id_or_id(&self) -> Uuid;
}

impl PaymentId for Payment {
    fn payment_id_or_id(&self) -> Uuid {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use super::accounts;
    use uuid::Uuid;

    #[test]
    fn account_ids_are_distinct() {
        let ids = [
            accounts::GATEWAY_FLOAT,
            accounts::MERCHANT_PAYABLE,
            accounts::FEE_INCOME,
            accounts::REFUND_RESERVE,
            accounts::CHARGEBACK_RESERVE,
        ];
        let unique: std::collections::HashSet<Uuid> = ids.iter().cloned().collect();
        assert_eq!(unique.len(), ids.len(), "Account IDs must be unique");
    }
}
