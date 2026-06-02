use chrono::Utc;
use common::{
    errors::{AppError, AppResult},
    models::{
        CaptureMethod, CreatePaymentRequest, CreateRefundRequest, Payment,
        PaymentMethod, PaymentMethodInput, PaymentStatus, CardDetails,
        UpiDetails, NetBankingDetails, Refund, RefundStatus,
    },
};
use serde_json::json;
use tracing::instrument;
use uuid::Uuid;

use crate::{
    db,
    ledger::LedgerWriter,
    metrics,
    state::AppState,
    state_machine::PaymentStateMachine,
};


const TOPIC_PAYMENTS: &str = "rustpay.payments";
const TOPIC_WEBHOOKS: &str = "rustpay.webhooks";

pub struct PaymentOrchestrator<'a> {
    state: &'a AppState,
}

impl<'a> PaymentOrchestrator<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    

    
    
    
    
    
    
    
    
    #[instrument(skip(self), fields(merchant_id = %merchant_id))]
    pub async fn create_payment(
        &self,
        merchant_id: Uuid,
        req: CreatePaymentRequest,
        idempotency_key: Option<String>,
    ) -> AppResult<Payment> {
        let now = Utc::now();
        let payment_id = Uuid::new_v4();
        let capture_method = req.capture_method.unwrap_or(CaptureMethod::Automatic);
        let payment_method = map_payment_method_input(req.payment_method);

        
        let idem_key = idempotency_key.or(req.idempotency_key);

        let payment = Payment {
            id: payment_id,
            merchant_id,
            order_id: req.order_id,
            amount: req.amount,
            currency: req.currency,
            status: PaymentStatus::Created,
            payment_method: Some(payment_method),
            description: req.description,
            metadata: req.metadata.unwrap_or(json!({})),
            acquirer_id: None,
            acquirer_reference: None,
            failure_code: None,
            failure_message: None,
            capture_method,
            captured_amount: None,
            refunded_amount: 0,
            idempotency_key: idem_key.clone(),
            created_at: now,
            updated_at: now,
            captured_at: None,
            settled_at: None,
            expires_at: Some(now + chrono::Duration::hours(24)),
        };

        
        let pool_start = std::time::Instant::now();
        let mut tx = self.state.db.begin().await?;
        metrics::record_db_pool_wait_duration("payment_service", pool_start.elapsed());

        let q_start = std::time::Instant::now();
        let (actual_id, was_inserted) =
            db::insert_payment_idempotent(&mut tx, &payment).await?;
        metrics::record_db_query_duration("insert_payment_idempotent", q_start.elapsed());

        if !was_inserted {
            
            tx.commit().await?;
            tracing::info!(
                idempotency_key = ?idem_key,
                existing_payment_id = %actual_id,
                "Idempotent replay — returning existing payment"
            );
            return db::find_payment_by_id(&self.state.db, actual_id, merchant_id)
                .await?
                .ok_or_else(|| AppError::Internal("Idempotency race".into()));
        }

        
        db::insert_outbox_event(
            &mut tx,
            "payment",
            payment.id,
            "payment.created",
            &build_payment_event_payload(&payment),
            TOPIC_PAYMENTS,
        )
        .await?;

        tx.commit().await?;
        metrics::record_payment_created(&payment.currency.to_string());
        tracing::info!(payment_id = %payment.id, "Payment created");

        
        match self.run_fraud_check(&payment).await {
            Err(AppError::FraudBlocked(reason)) => {
                tracing::warn!(payment_id = %payment.id, reason = %reason, "Fraud blocked");
                let mut tx = self.state.db.begin().await?;
                db::update_payment_status(
                    &mut tx,
                    payment.id,
                    PaymentStatus::Failed,
                    Some("fraud_blocked".into()),
                    Some(reason.clone()),
                )
                .await?;
                db::insert_outbox_event(
                    &mut tx,
                    "payment",
                    payment.id,
                    "payment.failed",
                    &json!({"payment_id": payment.id, "reason": "fraud_blocked"}),
                    TOPIC_PAYMENTS,
                )
                .await?;
                tx.commit().await?;
                metrics::record_payment_failed("fraud_blocked");
                return Err(AppError::FraudBlocked(reason));
            }
            Err(e) => {
                
                tracing::error!(
                    payment_id = %payment.id,
                    error = %e,
                    "Fraud service error — proceeding with payment (fail-open policy)"
                );
            }
            Ok(()) => {}
        }

        
        match self.charge_via_acquirer(&payment).await {
            Ok((acquirer_id, acquirer_ref)) => {
                
                let pool_start = std::time::Instant::now();
                let mut tx = self.state.db.begin().await?;
                metrics::record_db_pool_wait_duration("payment_service", pool_start.elapsed());
                
                let q_start = std::time::Instant::now();
                let mut locked = db::lock_payment_for_update(
                    &mut tx, payment.id, merchant_id
                ).await?;
                metrics::record_db_query_duration("lock_payment", q_start.elapsed());

                let mut sm = PaymentStateMachine::new(locked.status);

                if capture_method == CaptureMethod::Automatic {
                    
                    sm.transition(PaymentStatus::Pending)?;
                    sm.transition(PaymentStatus::Authorized)?;
                    sm.transition(PaymentStatus::Captured)?;
                    locked.status = PaymentStatus::Captured;
                    locked.captured_at = Some(Utc::now());
                    locked.captured_amount = Some(payment.amount);
                } else {
                    sm.transition(PaymentStatus::Pending)?;
                    sm.transition(PaymentStatus::Authorized)?;
                    locked.status = PaymentStatus::Authorized;
                }

                locked.acquirer_id = Some(acquirer_id);
                locked.acquirer_reference = Some(acquirer_ref);
                locked.updated_at = Utc::now();

                db::update_payment_after_charge(&mut tx, &locked).await?;

                
                if locked.status == PaymentStatus::Captured {
                    let mut ledger = LedgerWriter::new(&mut tx);
                    ledger.record_capture(&locked).await?;

                    
                    let fee = (locked.amount as f64 * self.state.config.system_fee_percentage) as i64;
                    if fee > 0 {
                        ledger.record_fee(&locked, fee).await?;
                    }
                }

                let event_type = if locked.status == PaymentStatus::Captured {
                    "payment.captured"
                } else {
                    "payment.authorized"
                };

                db::insert_outbox_event(
                    &mut tx,
                    "payment",
                    locked.id,
                    event_type,
                    &build_payment_event_payload(&locked),
                    TOPIC_PAYMENTS,
                )
                .await?;

                
                db::insert_outbox_event(
                    &mut tx,
                    "payment",
                    locked.id,
                    event_type,
                    &build_payment_event_payload(&locked),
                    TOPIC_WEBHOOKS,
                )
                .await?;

                tx.commit().await?;

                if locked.status == PaymentStatus::Captured {
                    metrics::record_payment_captured(
                        &locked.currency.to_string(),
                        locked.captured_amount.unwrap_or(locked.amount),
                    );
                }

                tracing::info!(
                    payment_id = %locked.id,
                    status = ?locked.status,
                    "Payment processed successfully"
                );
                Ok(locked)
            }

            Err(AppError::AcquirerDeclined { code, message }) => {
                let mut tx = self.state.db.begin().await?;
                db::update_payment_status(
                    &mut tx,
                    payment.id,
                    PaymentStatus::Failed,
                    Some(code.clone()),
                    Some(message.clone()),
                )
                .await?;
                db::insert_outbox_event(
                    &mut tx,
                    "payment",
                    payment.id,
                    "payment.failed",
                    &json!({
                        "payment_id": payment.id,
                        "reason": "acquirer_declined",
                        "code": code,
                        "message": message
                    }),
                    TOPIC_PAYMENTS,
                )
                .await?;
                tx.commit().await?;
                metrics::record_payment_failed("acquirer_declined");
                Err(AppError::AcquirerDeclined { code, message })
            }

            Err(e) => {
                let mut tx = self.state.db.begin().await?;
                db::update_payment_status(
                    &mut tx,
                    payment.id,
                    PaymentStatus::Failed,
                    Some("processing_error".into()),
                    Some(e.to_string()),
                )
                .await?;
                db::insert_outbox_event(
                    &mut tx,
                    "payment",
                    payment.id,
                    "payment.failed",
                    &json!({"payment_id": payment.id, "reason": "processing_error"}),
                    TOPIC_PAYMENTS,
                )
                .await?;
                tx.commit().await?;
                metrics::record_payment_failed("processing_error");
                Err(e)
            }
        }
    }

    

    
    
    
    
    
    
    #[instrument(skip(self), fields(payment_id = %payment_id))]
    pub async fn capture_payment(
        &self,
        payment_id: Uuid,
        merchant_id: Uuid,
        capture_amount: Option<i64>,
    ) -> AppResult<Payment> {
        
        let payment = db::find_payment_by_id(&self.state.db, payment_id, merchant_id)
            .await?
            .ok_or_else(|| AppError::Internal("Payment not found".into()))?;

        {
            let mut sm = PaymentStateMachine::new(payment.status);
            sm.transition(PaymentStatus::Captured)?;
        }

        
        self.acquirer_capture(payment_id, payment.idempotency_key).await?;

        
        let pool_start = std::time::Instant::now();
        let mut tx = self.state.db.begin().await?;
        metrics::record_db_pool_wait_duration("payment_service", pool_start.elapsed());
        
        let q_start = std::time::Instant::now();
        let mut payment =
            db::lock_payment_for_update(&mut tx, payment_id, merchant_id).await?;
        metrics::record_db_query_duration("lock_payment", q_start.elapsed());

        let mut sm = PaymentStateMachine::new(payment.status);
        payment.status = sm.transition(PaymentStatus::Captured)?;
        payment.captured_at = Some(Utc::now());
        payment.captured_amount = Some(capture_amount.unwrap_or(payment.amount));
        payment.updated_at = Utc::now();

        db::update_payment_after_charge(&mut tx, &payment).await?;

        
        let mut ledger = LedgerWriter::new(&mut tx);
        ledger.record_capture(&payment).await?;
        let fee = (payment.captured_amount.unwrap_or(payment.amount) as f64 * self.state.config.system_fee_percentage) as i64;
        if fee > 0 {
            ledger.record_fee(&payment, fee).await?;
        }

        db::insert_outbox_event(
            &mut tx,
            "payment",
            payment.id,
            "payment.captured",
            &build_payment_event_payload(&payment),
            TOPIC_PAYMENTS,
        )
        .await?;
        db::insert_outbox_event(
            &mut tx,
            "payment",
            payment.id,
            "payment.captured",
            &build_payment_event_payload(&payment),
            TOPIC_WEBHOOKS,
        )
        .await?;

        tx.commit().await?;

        metrics::record_payment_captured(
            &payment.currency.to_string(),
            payment.captured_amount.unwrap_or(payment.amount),
        );
        tracing::info!(payment_id = %payment.id, "Payment captured");
        Ok(payment)
    }

    

    
    
    
    
    
    
    
    #[instrument(skip(self), fields(payment_id = %payment_id))]
    pub async fn create_refund(
        &self,
        payment_id: Uuid,
        merchant_id: Uuid,
        req: CreateRefundRequest,
    ) -> AppResult<Refund> {
        let now = Utc::now();
        let refund_id = Uuid::new_v4();

        let pool_start = std::time::Instant::now();
        let mut tx = self.state.db.begin().await?;
        metrics::record_db_pool_wait_duration("payment_service", pool_start.elapsed());
        
        let q_start = std::time::Instant::now();
        let payment = db::lock_payment_for_update(&mut tx, payment_id, merchant_id).await?;
        metrics::record_db_query_duration("lock_payment", q_start.elapsed());

        
        if !matches!(payment.status, PaymentStatus::Captured | PaymentStatus::Settled) {
            tx.rollback().await?;
            return Err(AppError::InvalidStateTransition {
                from: format!("{:?}", payment.status),
                to: "Refund".into(),
            });
        }

        let refund_amount = req.amount.unwrap_or_else(|| payment.refundable_amount());

        if !payment.can_refund(refund_amount) {
            tx.rollback().await?;
            return Err(AppError::AmountExceedsLimit {
                amount: refund_amount,
                max: payment.refundable_amount(),
            });
        }

        let refund = Refund {
            id: refund_id,
            payment_id,
            merchant_id,
            amount: refund_amount,
            currency: payment.currency,
            status: RefundStatus::Pending,
            reason: req.reason,
            acquirer_refund_id: None,
            failure_reason: None,
            idempotency_key: req.idempotency_key,
            created_at: now,
            updated_at: now,
        };

        db::insert_refund(&mut tx, &refund).await?;
        db::increment_refunded_amount(&mut tx, payment_id, refund_amount).await?;

        
        let mut ledger = LedgerWriter::new(&mut tx);
        ledger.record_refund(&refund).await?;

        db::insert_outbox_event(
            &mut tx,
            "refund",
            refund.id,
            "refund.pending",
            &build_refund_event_payload(&refund),
            TOPIC_PAYMENTS,
        )
        .await?;

        tx.commit().await?;
        tracing::info!(refund_id = %refund.id, amount = refund_amount, "Refund created");

        
        match self.acquirer_refund(payment_id, refund_amount).await {
            Ok(acquirer_refund_id) => {
                db::update_refund_status(
                    &self.state.db,
                    refund.id,
                    RefundStatus::Succeeded,
                    Some(acquirer_refund_id),
                    None,
                )
                .await?;

                
                let mut tx = self.state.db.begin().await?;
                db::insert_outbox_event(
                    &mut tx,
                    "refund",
                    refund.id,
                    "refund.succeeded",
                    &build_refund_event_payload(&refund),
                    TOPIC_WEBHOOKS,
                )
                .await?;
                tx.commit().await?;

                metrics::record_refund(&refund.currency.to_string());
                tracing::info!(refund_id = %refund.id, "Refund succeeded");

                let mut succeeded_refund = refund;
                succeeded_refund.status = RefundStatus::Succeeded;
                Ok(succeeded_refund)
            }
            Err(e) => {
                let reason = e.to_string();
                db::update_refund_status(
                    &self.state.db,
                    refund.id,
                    RefundStatus::Failed,
                    None,
                    Some(reason.clone()),
                )
                .await?;
                tracing::error!(refund_id = %refund.id, error = %reason, "Refund failed at acquirer");

                
                
                
                Err(AppError::AcquirerUnavailable(format!("Refund failed: {reason}")))
            }
        }
    }

    

    
    #[instrument(skip(self), fields(payment_id = %payment_id))]
    pub async fn cancel_payment(
        &self,
        payment_id: Uuid,
        merchant_id: Uuid,
    ) -> AppResult<Payment> {
        let mut tx = self.state.db.begin().await?;
        let mut payment =
            db::lock_payment_for_update(&mut tx, payment_id, merchant_id).await?;

        let mut sm = PaymentStateMachine::new(payment.status);
        payment.status = sm.transition(PaymentStatus::Cancelled)?;
        payment.updated_at = Utc::now();

        db::update_payment_status(
            &mut tx,
            payment.id,
            PaymentStatus::Cancelled,
            None,
            None,
        )
        .await?;

        db::insert_outbox_event(
            &mut tx,
            "payment",
            payment.id,
            "payment.cancelled",
            &build_payment_event_payload(&payment),
            TOPIC_PAYMENTS,
        )
        .await?;

        tx.commit().await?;
        tracing::info!(payment_id = %payment.id, "Payment cancelled");
        Ok(payment)
    }

    

    async fn run_fraud_check(&self, payment: &Payment) -> AppResult<()> {
        let url = format!("{}/v1/risk/evaluate", self.state.config.fraud_service_url);
        let resp = self
            .state
            .http_client
            .post(&url)
            .json(&json!({
                "payment_id": payment.id,
                "merchant_id": payment.merchant_id,
                "amount": payment.amount,
                "currency": payment.currency,
                "payment_method": payment.payment_method,
            }))
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("Fraud service unreachable: {e}")))?;

        if resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            if body["blocked"].as_bool().unwrap_or(false) {
                let reason = body["reason"].as_str().unwrap_or("unknown").to_string();
                return Err(AppError::FraudBlocked(reason));
            }
        }
        Ok(())
    }

    async fn charge_via_acquirer(
        &self,
        payment: &Payment,
    ) -> AppResult<(String, String)> {
        let url = format!("{}/v1/charge", self.state.config.acquirer_router_url);
        let resp = self
            .state
            .http_client
            .post(&url)
            .json(&json!({
                "payment_id": payment.id,
                "amount": payment.amount,
                "currency": payment.currency,
                "payment_method": payment.payment_method,
                "capture": payment.capture_method == CaptureMethod::Automatic,
            }))
            .send()
            .await
            .map_err(|e| AppError::AcquirerUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            return Err(AppError::AcquirerDeclined {
                code: body["code"].as_str().unwrap_or("declined").to_string(),
                message: body["message"]
                    .as_str()
                    .unwrap_or("Payment declined")
                    .to_string(),
            });
        }

        let body: serde_json::Value =
            resp.json().await.map_err(|e| AppError::Internal(e.to_string()))?;

        Ok((
            body["acquirer_id"].as_str().unwrap_or("unknown").to_string(),
            body["reference"].as_str().unwrap_or("").to_string(),
        ))
    }

    async fn acquirer_capture(&self, payment_id: Uuid, idempotency_key: Option<String>) -> AppResult<()> {
        let url = format!("{}/v1/capture", self.state.config.acquirer_router_url);
        let idem_key = idempotency_key.unwrap_or_else(|| format!("capture-{}", payment_id));

        let resp = self
            .state
            .http_client
            .post(&url)
            .header("X-Idempotency-Key", idem_key)
            .json(&json!({ "payment_id": payment_id }))
            .send()
            .await
            .map_err(|e| AppError::AcquirerUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AppError::AcquirerUnavailable(
                "Capture call failed".into(),
            ));
        }
        Ok(())
    }

    async fn acquirer_refund(
        &self,
        payment_id: Uuid,
        amount: i64,
    ) -> AppResult<String> {
        let url = format!("{}/v1/refund", self.state.config.acquirer_router_url);
        let resp = self
            .state
            .http_client
            .post(&url)
            .json(&json!({ "payment_id": payment_id, "amount": amount }))
            .send()
            .await
            .map_err(|e| AppError::AcquirerUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AppError::AcquirerUnavailable("Refund call failed".into()));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        Ok(body["refund_id"].as_str().unwrap_or("").to_string())
    }
}



fn build_payment_event_payload(payment: &Payment) -> serde_json::Value {
    json!({
        "event_id": Uuid::new_v4(),
        "payment_id": payment.id,
        "merchant_id": payment.merchant_id,
        "order_id": payment.order_id,
        "amount": payment.amount,
        "captured_amount": payment.captured_amount,
        "currency": payment.currency,
        "status": payment.status,
        "acquirer_id": payment.acquirer_id,
        "acquirer_reference": payment.acquirer_reference,
        "timestamp": Utc::now(),
    })
}

fn build_refund_event_payload(refund: &Refund) -> serde_json::Value {
    json!({
        "event_id": Uuid::new_v4(),
        "refund_id": refund.id,
        "payment_id": refund.payment_id,
        "merchant_id": refund.merchant_id,
        "amount": refund.amount,
        "currency": refund.currency,
        "status": refund.status,
        "reason": refund.reason,
        "timestamp": Utc::now(),
    })
}



fn map_payment_method_input(input: PaymentMethodInput) -> PaymentMethod {
    match input {
        PaymentMethodInput::Card { token, last4, brand, exp_month, exp_year, name } => {
            PaymentMethod::Card(CardDetails {
                token,
                last4,
                brand,
                exp_month,
                exp_year,
                name,
                country: None,
            })
        }
        PaymentMethodInput::Upi { vpa } => {
            PaymentMethod::Upi(UpiDetails { vpa, bank_name: None })
        }
        PaymentMethodInput::NetBanking { bank_code } => {
            PaymentMethod::NetBanking(NetBankingDetails {
                bank_name: bank_code.clone(),
                bank_code,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PaymentServiceConfig;
    use crate::state::AppState;
    use common::models::{Currency, CaptureMethod, PaymentMethodInput, PaymentStatus};
    use sqlx::postgres::PgPoolOptions;
    use deadpool_redis::{Config as RedisConfig, Runtime};
    use uuid::Uuid;
    use mockito::Server;
    use std::sync::Arc;

    async fn setup_app_state(mock_server_url: String) -> AppState {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .unwrap_or_else(|_| "postgres://rustpay:rustpay_secret@localhost:5432/rustpay".into());
        
        let db = PgPoolOptions::new()
            .max_connections(2)
            .connect(&db_url)
            .await
            .expect("Failed to connect to DB");

        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://:rustpay_redis_secret@localhost:6379/0".into());
        
        let cfg = RedisConfig::from_url(redis_url);
        let redis = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();

        let config = PaymentServiceConfig {
            server: common::config::ServerConfig { host: "0".into(), port: 0 },
            database: common::config::DatabaseConfig { url: db_url, max_connections: 2, min_connections: 1, connect_timeout_secs: 5 },
            redis: common::config::RedisConfig { url: "redis".into(), max_connections: 2 },
            kafka: common::config::KafkaConfig {
                bootstrap_servers: "localhost:9092".into(),
                consumer_group_id: "test".into(),
                topic_payments: "test.payments".into(),
                topic_merchants: "test.merchants".into(),
                topic_webhooks: "test.webhooks".into(),
                topic_ledger: "test.ledger".into(),
                topic_fraud: "test.fraud".into(),
            },
            telemetry: common::config::TelemetryConfig { otlp_endpoint: "".into(), service_name: "".into() },
            idempotency_ttl_seconds: 3600,
            vault_service_url: mock_server_url.clone(),
            acquirer_router_url: mock_server_url.clone(),
            fraud_service_url: mock_server_url.clone(),
            system_fee_percentage: 0.02,
        };

        AppState::new(db, redis, config)
    }

    #[tokio::test]
    async fn test_create_payment_fail_open_fraud_and_successful_charge() {
        let mut server = Server::new_async().await;
        
        // Fraud service mock (returns 500 to test fail-open policy)
        let _fraud_mock = server.mock("POST", "/v1/risk/evaluate")
            .with_status(500)
            .create_async()
            .await;

        // Acquirer mock
        let _acquirer_mock = server.mock("POST", "/v1/charge")
            .with_status(200)
            .with_body(r#"{"acquirer_id": "acq_123", "reference": "ref_456"}"#)
            .create_async()
            .await;

        let state = setup_app_state(server.url()).await;
        let orchestrator = PaymentOrchestrator::new(&state);
        let merchant_id = Uuid::new_v4();

        // Ensure merchant exists in test db (merchants must exist for foreign keys if any)
        // Note: The schema might require a merchant in `merchants` table first. Let's try inserting one.
        let _ = sqlx::query!(
            r#"
            INSERT INTO merchants (id, business_name, email, kyc_status, api_key_hash, test_api_key_hash, is_active)
            VALUES ($1, 'Test Merchant', 'test@example.com', 'approved', 'hash', 'testhash', true)
            ON CONFLICT (id) DO NOTHING
            "#,
            merchant_id
        ).execute(&state.db).await;

        let req = CreatePaymentRequest {
            amount: 1000,
            currency: Currency::INR,
            payment_method: PaymentMethodInput::Upi { vpa: "test@upi".into() },
            order_id: None,
            description: None,
            metadata: None,
            capture_method: Some(CaptureMethod::Automatic),
            idempotency_key: Some(Uuid::new_v4().to_string()),
        };

        let result = orchestrator.create_payment(merchant_id, req, None).await;
        assert!(result.is_ok(), "Payment creation failed: {:?}", result.err());

        let payment = result.unwrap();
        assert_eq!(payment.status, PaymentStatus::Captured);
        assert_eq!(payment.amount, 1000);
        assert_eq!(payment.acquirer_id.as_deref(), Some("acq_123"));
    }
}

