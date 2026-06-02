use common::{config::KafkaConfig, errors::AppResult, models::Payment};
use rdkafka::{
    producer::{FutureProducer, FutureRecord},
    ClientConfig,
};
use std::time::Duration;
use tracing::instrument;

pub struct PaymentEventPublisher {
    producer: FutureProducer,
    topic: String,
    webhook_topic: String,
}

impl PaymentEventPublisher {
    pub fn new(cfg: &KafkaConfig) -> Self {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &cfg.bootstrap_servers)
            .set("message.timeout.ms", "5000")
            .set("acks", "all")
            .set("enable.idempotence", "true")
            .create()
            .expect("Failed to create Kafka producer");

        Self {
            producer,
            topic: cfg.topic_payments.clone(),
            webhook_topic: cfg.topic_webhooks.clone(),
        }
    }

    
    #[instrument(skip(self), fields(payment_id = %payment.id, status = ?payment.status))]
    pub async fn publish_payment_event(&self, payment: &Payment) -> AppResult<()> {
        let event = PaymentEvent::from(payment);
        let payload = serde_json::to_string(&event)
            .map_err(common::errors::AppError::Serialization)?;

        let key = payment.id.to_string();
        let record = FutureRecord::to(&self.topic)
            .key(&key)
            .payload(&payload);

        self.producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| {
                common::errors::AppError::Messaging(format!("Kafka send failed: {e}"))
            })?;

        tracing::debug!(payment_id = %payment.id, topic = %self.topic, "Payment event published");

        
        let webhook_event = WebhookEvent {
            event_type: event_type_for_status(payment),
            payment_id: payment.id,
            merchant_id: payment.merchant_id,
            payload: serde_json::to_value(payment).unwrap_or_default(),
        };
        let webhook_payload = serde_json::to_string(&webhook_event)
            .map_err(common::errors::AppError::Serialization)?;

        let webhook_record = FutureRecord::to(&self.webhook_topic)
            .key(&key)
            .payload(&webhook_payload);

        if let Err((e, _)) = self.producer.send(webhook_record, Duration::from_secs(5)).await {
            tracing::warn!(error = %e, "Failed to publish webhook event");
        }

        Ok(())
    }
}

#[derive(serde::Serialize)]
struct PaymentEvent {
    event_id: uuid::Uuid,
    event_type: String,
    payment_id: uuid::Uuid,
    merchant_id: uuid::Uuid,
    amount: i64,
    currency: String,
    status: String,
    timestamp: chrono::DateTime<chrono::Utc>,
}

impl From<&Payment> for PaymentEvent {
    fn from(p: &Payment) -> Self {
        Self {
            event_id: uuid::Uuid::new_v4(),
            event_type: event_type_for_status(p).to_string(),
            payment_id: p.id,
            merchant_id: p.merchant_id,
            amount: p.amount,
            currency: p.currency.to_string(),
            status: format!("{:?}", p.status),
            timestamp: chrono::Utc::now(),
        }
    }
}

#[derive(serde::Serialize)]
struct WebhookEvent {
    event_type: &'static str,
    payment_id: uuid::Uuid,
    merchant_id: uuid::Uuid,
    payload: serde_json::Value,
}

fn event_type_for_status(payment: &Payment) -> &'static str {
    use common::models::PaymentStatus::*;
    match payment.status {
        Created => "payment.created",
        Authorized => "payment.authorized",
        Captured => "payment.captured",
        Settled => "payment.settled",
        Failed => "payment.failed",
        Refunded => "payment.refunded",
        Disputed => "payment.disputed",
        Cancelled => "payment.cancelled",
        Pending | RequiresAction => "payment.pending",
    }
}
