-- Outbox events: add locked_at for lease timeout and crash recovery
ALTER TABLE outbox_events ADD COLUMN locked_at TIMESTAMPTZ;

-- Webhook deliveries: add processing_started_at for lease timeout and crash recovery
ALTER TABLE webhook_deliveries ADD COLUMN processing_started_at TIMESTAMPTZ;
