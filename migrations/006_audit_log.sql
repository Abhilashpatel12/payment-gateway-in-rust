


CREATE TABLE audit_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_type      TEXT NOT NULL,  
    actor_id        UUID NOT NULL,
    event           TEXT NOT NULL,  
    resource_type   TEXT NOT NULL,
    resource_id     UUID NOT NULL,
    changes         JSONB,          
    ip_address      INET,
    user_agent      TEXT,
    
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


CREATE RULE audit_no_update AS ON UPDATE TO audit_log DO INSTEAD NOTHING;
CREATE RULE audit_no_delete AS ON DELETE TO audit_log DO INSTEAD NOTHING;

CREATE INDEX idx_audit_log_actor ON audit_log(actor_id, actor_type);
CREATE INDEX idx_audit_log_resource ON audit_log(resource_type, resource_id);
CREATE INDEX idx_audit_log_event ON audit_log(event);
CREATE INDEX idx_audit_log_created_at ON audit_log(created_at DESC);


CREATE TABLE vault_tokens (
    token           TEXT PRIMARY KEY,
    
    encrypted_data  TEXT NOT NULL,
    
    fingerprint     TEXT NOT NULL,
    last4           TEXT NOT NULL,
    card_brand      card_brand NOT NULL,
    exp_month       SMALLINT NOT NULL,
    exp_year        SMALLINT NOT NULL,
    merchant_id     UUID REFERENCES merchants(id),
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at    TIMESTAMPTZ
);


CREATE RULE vault_no_update AS ON UPDATE TO vault_tokens DO INSTEAD NOTHING;

CREATE UNIQUE INDEX idx_vault_fingerprint_merchant
    ON vault_tokens(fingerprint, merchant_id)
    WHERE is_active = true;
