use ::metrics::{counter, gauge, histogram};

pub fn record_payment_created(currency: &str) {
    counter!("payments_created_total", "currency" => currency.to_owned()).increment(1);
}

pub fn record_payment_captured(currency: &str, amount_minor: i64) {
    counter!("payments_captured_total", "currency" => currency.to_owned()).increment(1);
    histogram!("payments_captured_amount_minor", "currency" => currency.to_owned())
        .record(amount_minor as f64);
}

pub fn record_payment_failed(reason: &str) {
    counter!("payments_failed_total", "reason" => reason.to_owned()).increment(1);
}

pub fn record_refund(currency: &str) {
    counter!("refunds_total", "currency" => currency.to_owned()).increment(1);
}

pub fn record_webhook_failure() {
    counter!("webhook_failures_total").increment(1);
}

pub fn record_outbox_lag_seconds(seconds: f64) {
    gauge!("outbox_lag_seconds").set(seconds);
}

pub fn record_ledger_entry() {
    counter!("ledger_entries_total").increment(1);
}

pub fn record_fraud_block() {
    counter!("fraud_blocks_total").increment(1);
}

pub fn record_dispute_opened() {
    counter!("disputes_opened_total").increment(1);
}

pub fn record_settlement_completed(currency: &str, amount_minor: i64) {
    counter!("settlements_completed_total", "currency" => currency.to_owned()).increment(1);
    histogram!("settlements_amount_minor", "currency" => currency.to_owned())
        .record(amount_minor as f64);
}

pub fn record_db_pool_wait_duration(pool_name: &str, duration: std::time::Duration) {
    histogram!("db_pool_wait_duration_seconds", "pool" => pool_name.to_owned())
        .record(duration.as_secs_f64());
}

pub fn record_db_query_duration(query_name: &str, duration: std::time::Duration) {
    histogram!("db_query_duration_seconds", "query" => query_name.to_owned())
        .record(duration.as_secs_f64());
}
