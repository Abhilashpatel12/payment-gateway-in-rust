use anyhow::Result;
use futures::StreamExt;
use rdkafka::{
    consumer::{CommitMode, Consumer, StreamConsumer},
    ClientConfig, Message,
};
use sqlx::PgPool;

use crate::{dispatcher::WebhookDispatcher, retry_scheduler::RetryScheduler};

const CONSUMER_GROUP: &str = "rustpay-webhook-service";

pub struct WebhookConsumer {
    consumer: StreamConsumer,
    db: PgPool,
    dispatcher: WebhookDispatcher,
    scheduler: RetryScheduler,
}

impl WebhookConsumer {
    pub fn new(db: PgPool, bootstrap_servers: &str, topic: &str) -> Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", bootstrap_servers)
            .set("group.id", CONSUMER_GROUP)
            .set("auto.offset.reset", "earliest")
            .set("enable.auto.commit", "false")
            
            .set("max.poll.interval.ms", "300000")
            .create()?;

        consumer.subscribe(&[topic])?;
        tracing::info!(topic, "Webhook consumer subscribed");

        let dispatcher = WebhookDispatcher::new(db.clone());
        let scheduler = RetryScheduler::new(db.clone());

        Ok(Self {
            consumer,
            db,
            dispatcher,
            scheduler,
        })
    }

    pub async fn run(self) -> Result<()> {
        
        let db_clone = self.db.clone();
        tokio::spawn(async move {
            loop {
                let sched = RetryScheduler::new(db_clone.clone());
                if let Err(e) = sched.run_once().await {
                    tracing::error!(error = %e, "Retry scheduler error");
                }
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            }
        });

        let mut stream = self.consumer.stream();

        while let Some(message) = stream.next().await {
            match message {
                Ok(msg) => {
                    let payload = msg.payload().unwrap_or_default();

                    
                    let event_id = format!(
                        "{}-{}-{}",
                        msg.topic(),
                        msg.partition(),
                        msg.offset()
                    );

                    match self.process_message(&event_id, payload).await {
                        Ok(skipped) => {
                            if skipped {
                                tracing::debug!(event_id, "Webhook event already processed — skipping");
                            }
                        }
                        Err(e) => {
                            tracing::error!(event_id, error = %e, "Failed to process webhook event");
                            
                            
                            continue;
                        }
                    }

                    
                    if let Err(e) = self.consumer.commit_message(&msg, CommitMode::Async) {
                        tracing::error!(error = %e, "Failed to commit Kafka offset");
                    }
                }
                Err(e) => {
                    tracing::error!(error = %e, "Kafka consumer error");
                }
            }
        }

        Ok(())
    }

    
    async fn process_message(&self, event_id: &str, payload: &[u8]) -> Result<bool> {
        
        let inserted = sqlx::query(
            r#"
            INSERT INTO processed_events (event_id, consumer_group)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(event_id)
        .bind(CONSUMER_GROUP)
        .execute(&self.db)
        .await?
        .rows_affected();

        if inserted == 0 {
            return Ok(true); 
        }

        let event: serde_json::Value = serde_json::from_slice(payload)?;

        
        
        self.dispatcher.persist_and_dispatch(event).await?;

        Ok(false)
    }
}

pub async fn start(db: PgPool, bootstrap_servers: String, topic: String) -> Result<()> {
    WebhookConsumer::new(db, &bootstrap_servers, &topic)?.run().await
}
