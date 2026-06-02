


CREATE TABLE payments (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id         UUID NOT NULL REFERENCES merchants(id) ON DELETE RESTRICT,
    order_id            UUID,   

    
    amount              BIGINT NOT NULL CHECK (amount > 0),
    currency            currency_code NOT NULL,

    
    status              payment_status NOT NULL DEFAULT 'created',
    capture_method      capture_method NOT NULL DEFAULT 'automatic',

    
    payment_method      JSONB,

    
    acquirer_id         TEXT,
    acquirer_reference  TEXT,

    
    failure_code        TEXT,
    failure_message     TEXT,

    
    description         TEXT,
    metadata            JSONB DEFAULT '{}',

    
    idempotency_key     TEXT,

    
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    captured_at         TIMESTAMPTZ,
    expires_at          TIMESTAMPTZ
);

CREATE TRIGGER payments_updated_at
    BEFORE UPDATE ON payments
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();


CREATE INDEX idx_payments_merchant_id ON payments(merchant_id);
CREATE INDEX idx_payments_status ON payments(status);
CREATE INDEX idx_payments_created_at ON payments(created_at DESC);
CREATE INDEX idx_payments_order_id ON payments(order_id) WHERE order_id IS NOT NULL;
CREATE UNIQUE INDEX idx_payments_idempotency
    ON payments(merchant_id, idempotency_key)
    WHERE idempotency_key IS NOT NULL;
CREATE INDEX idx_payments_acquirer_reference
    ON payments(acquirer_reference) WHERE acquirer_reference IS NOT NULL;
