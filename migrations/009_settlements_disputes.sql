
CREATE TYPE settlement_status AS ENUM (
    'pending',      
    'processing',   
    'completed',    
    'failed'        
);



ALTER TABLE settlements
    ADD COLUMN IF NOT EXISTS settlement_status settlement_status NOT NULL DEFAULT 'pending',
    ADD COLUMN IF NOT EXISTS fee_amount   BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS net_amount   BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS settled_payments UUID[] DEFAULT '{}';


CREATE INDEX IF NOT EXISTS idx_settlements_pending
    ON settlements(merchant_id, created_at)
    WHERE settlement_status = 'pending';




ALTER TABLE payments
    ADD COLUMN IF NOT EXISTS captured_amount BIGINT,
    ADD COLUMN IF NOT EXISTS refunded_amount BIGINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS settled_at      TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_payments_unsettled
    ON payments(merchant_id, captured_at)
    WHERE status = 'captured' AND settled_at IS NULL;


CREATE TYPE dispute_status AS ENUM (
    'warning_needs_response',   
    'warning_under_review',     
    'warning_closed',           
    'needs_response',           
    'under_review',             
    'charge_refunded',          
    'won',                      
    'lost'                      
);

CREATE TABLE disputes (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id          UUID NOT NULL REFERENCES payments(id) ON DELETE RESTRICT,
    merchant_id         UUID NOT NULL REFERENCES merchants(id) ON DELETE RESTRICT,

    amount              BIGINT NOT NULL CHECK (amount > 0),
    currency            currency_code NOT NULL,

    status              dispute_status NOT NULL DEFAULT 'needs_response',

    
    reason_code         TEXT NOT NULL,
    reason_description  TEXT,

    
    evidence            JSONB,         
    evidence_due_by     TIMESTAMPTZ,   
    evidence_submitted_at TIMESTAMPTZ,

    
    resolution          TEXT,          

    
    acquirer_dispute_id TEXT,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER disputes_updated_at
    BEFORE UPDATE ON disputes
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE INDEX idx_disputes_payment_id  ON disputes(payment_id);
CREATE INDEX idx_disputes_merchant_id ON disputes(merchant_id);
CREATE INDEX idx_disputes_status      ON disputes(status);
CREATE INDEX idx_disputes_evidence_due
    ON disputes(evidence_due_by)
    WHERE status IN ('needs_response', 'warning_needs_response');


CREATE TABLE exchange_rates (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_currency currency_code NOT NULL,
    to_currency   currency_code NOT NULL,
    rate          NUMERIC(20, 8) NOT NULL CHECK (rate > 0),
    source        TEXT NOT NULL DEFAULT 'manual',  
    valid_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (from_currency, to_currency, valid_at)
);

CREATE INDEX idx_exchange_rates_pair ON exchange_rates(from_currency, to_currency, valid_at DESC);


CREATE TABLE fraud_checks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id      UUID NOT NULL REFERENCES payments(id),
    merchant_id     UUID NOT NULL REFERENCES merchants(id),
    risk_score      INTEGER NOT NULL CHECK (risk_score BETWEEN 0 AND 100),
    blocked         BOOLEAN NOT NULL DEFAULT false,
    triggered_rules TEXT[] NOT NULL DEFAULT '{}',
    decision        TEXT NOT NULL,   
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_fraud_checks_payment ON fraud_checks(payment_id);
CREATE INDEX idx_fraud_checks_blocked ON fraud_checks(merchant_id, created_at)
    WHERE blocked = true;
