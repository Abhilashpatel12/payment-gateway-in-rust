
CREATE TABLE outbox_events (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    aggregate_type  TEXT NOT NULL,         
    aggregate_id    UUID NOT NULL,
    event_type      TEXT NOT NULL,         
    payload         JSONB NOT NULL,
    topic           TEXT NOT NULL,         
    published       BOOLEAN NOT NULL DEFAULT false,
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at    TIMESTAMPTZ
);


CREATE INDEX idx_outbox_unpublished ON outbox_events(created_at ASC)
    WHERE published = false AND failed_attempts < 10;

CREATE INDEX idx_outbox_aggregate ON outbox_events(aggregate_id, event_type);





CREATE TYPE refund_status AS ENUM (
    'pending',      
    'processing',   
    'succeeded',    
    'failed'        
);

CREATE TABLE refunds (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id          UUID NOT NULL REFERENCES payments(id) ON DELETE RESTRICT,
    merchant_id         UUID NOT NULL REFERENCES merchants(id) ON DELETE RESTRICT,

    
    amount              BIGINT NOT NULL CHECK (amount > 0),
    currency            currency_code NOT NULL,

    status              refund_status NOT NULL DEFAULT 'pending',

    
    reason              TEXT,

    
    acquirer_refund_id  TEXT,

    
    failure_reason      TEXT,

    
    idempotency_key     TEXT,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER refunds_updated_at
    BEFORE UPDATE ON refunds
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE INDEX idx_refunds_payment_id  ON refunds(payment_id);
CREATE INDEX idx_refunds_merchant_id ON refunds(merchant_id);
CREATE INDEX idx_refunds_status      ON refunds(status);
CREATE UNIQUE INDEX idx_refunds_idempotency
    ON refunds(merchant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;


CREATE TABLE processed_events (
    event_id        TEXT NOT NULL,         
    consumer_group  TEXT NOT NULL,         
    processed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (event_id, consumer_group)
);


CREATE INDEX idx_processed_events_at ON processed_events(processed_at);


ALTER TABLE merchants ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 1;


CREATE OR REPLACE FUNCTION increment_version_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.version = OLD.version + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER merchants_version
    BEFORE UPDATE ON merchants
    FOR EACH ROW EXECUTE FUNCTION increment_version_column();



CREATE INDEX IF NOT EXISTS idx_payments_merchant_status_created
    ON payments(merchant_id, status, created_at DESC);
