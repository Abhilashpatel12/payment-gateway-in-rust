







CREATE TABLE ledger_accounts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL UNIQUE,
    
    
    
    
    
    
    type        TEXT NOT NULL CHECK (type IN ('asset','liability','revenue','expense','equity')),
    currency    currency_code NOT NULL DEFAULT 'INR',
    description TEXT,
    is_system   BOOLEAN NOT NULL DEFAULT true,  
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);




INSERT INTO ledger_accounts (id, name, type, description) VALUES
    ('10000000-0000-0000-0000-000000000001', 'gateway_float',     'asset',    'Funds held by gateway pending settlement'),
    ('20000000-0000-0000-0000-000000000001', 'merchant_payable',  'liability','Funds owed to merchants'),
    ('30000000-0000-0000-0000-000000000001', 'fee_income',        'revenue',  'Platform transaction fees'),
    ('40000000-0000-0000-0000-000000000001', 'refund_reserve',    'liability','Funds reserved for potential refunds'),
    ('50000000-0000-0000-0000-000000000001', 'chargeback_reserve','liability','Funds reserved for chargeback losses');





ALTER TABLE ledger_entries
    ADD COLUMN IF NOT EXISTS account_id         UUID REFERENCES ledger_accounts(id),
    ADD COLUMN IF NOT EXISTS reference_entry_id UUID REFERENCES ledger_entries(id),
    ADD COLUMN IF NOT EXISTS notes              TEXT;


CREATE INDEX IF NOT EXISTS idx_ledger_account_merchant
    ON ledger_entries(account_id, merchant_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_ledger_reference
    ON ledger_entries(reference_entry_id)
    WHERE reference_entry_id IS NOT NULL;



CREATE TABLE reconciliation_runs (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider            TEXT NOT NULL,    
    period_start        TIMESTAMPTZ NOT NULL,
    period_end          TIMESTAMPTZ NOT NULL,
    status              TEXT NOT NULL DEFAULT 'running',  
    payments_checked    INTEGER NOT NULL DEFAULT 0,
    mismatches_found    INTEGER NOT NULL DEFAULT 0,
    error               TEXT,
    started_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at        TIMESTAMPTZ
);

CREATE TABLE reconciliation_mismatches (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id          UUID NOT NULL REFERENCES reconciliation_runs(id),
    payment_id      UUID REFERENCES payments(id),
    
    mismatch_type   TEXT NOT NULL,  
    our_value       TEXT,
    provider_value  TEXT,
    notes           TEXT,
    resolved_at     TIMESTAMPTZ,
    resolved_by     TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_recon_mismatches_run ON reconciliation_mismatches(run_id);
CREATE INDEX idx_recon_mismatches_payment ON reconciliation_mismatches(payment_id);
CREATE INDEX idx_recon_mismatches_unresolved ON reconciliation_mismatches(created_at)
    WHERE resolved_at IS NULL;
