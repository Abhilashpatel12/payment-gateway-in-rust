


CREATE TABLE ledger_entries (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    payment_id      UUID NOT NULL REFERENCES payments(id) ON DELETE RESTRICT,
    merchant_id     UUID NOT NULL REFERENCES merchants(id) ON DELETE RESTRICT,

    entry_type      entry_type NOT NULL,

    amount          BIGINT NOT NULL CHECK (amount > 0),
    currency        currency_code NOT NULL,

    
    balance_before  BIGINT NOT NULL,
    balance_after   BIGINT NOT NULL,

    description     TEXT NOT NULL,

    
    hash            TEXT NOT NULL,

    
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


CREATE RULE ledger_no_update AS ON UPDATE TO ledger_entries DO INSTEAD NOTHING;
CREATE RULE ledger_no_delete AS ON DELETE TO ledger_entries DO INSTEAD NOTHING;

CREATE INDEX idx_ledger_merchant_id ON ledger_entries(merchant_id);
CREATE INDEX idx_ledger_payment_id ON ledger_entries(payment_id);
CREATE INDEX idx_ledger_created_at ON ledger_entries(created_at DESC);


CREATE TABLE merchant_balances (
    merchant_id     UUID PRIMARY KEY REFERENCES merchants(id),
    currency        currency_code NOT NULL DEFAULT 'INR',
    available       BIGINT NOT NULL DEFAULT 0,    
    pending         BIGINT NOT NULL DEFAULT 0,    
    reserved        BIGINT NOT NULL DEFAULT 0,    
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


CREATE TABLE settlements (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id         UUID NOT NULL REFERENCES merchants(id),
    amount              BIGINT NOT NULL,
    currency            currency_code NOT NULL,
    status              TEXT NOT NULL DEFAULT 'pending',  
    payment_count       INTEGER NOT NULL DEFAULT 0,
    bank_reference      TEXT,
    initiated_at        TIMESTAMPTZ,
    completed_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_settlements_merchant_id ON settlements(merchant_id);
CREATE INDEX idx_settlements_status ON settlements(status);
