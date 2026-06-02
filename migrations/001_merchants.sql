CREATE TYPE currency_code AS ENUM (
    'INR', 'USD', 'EUR', 'GBP', 'AED', 'SGD'
);

CREATE TYPE payment_status AS ENUM (
    'created',
    'pending',
    'authorized',
    'captured',
    'settled',
    'failed',
    'refunded',
    'disputed',
    'cancelled',
    'requires_action'
);

CREATE TYPE capture_method AS ENUM ('automatic', 'manual');

CREATE TYPE payment_method_type AS ENUM (
    'card', 'upi', 'net_banking', 'wallet'
);

CREATE TYPE card_brand AS ENUM (
    'visa', 'mastercard', 'amex', 'discover', 'rupay', 'unknown'
);

CREATE TYPE kyc_status AS ENUM (
    'pending', 'under_review', 'approved', 'rejected', 'requires_documents'
);

CREATE TYPE order_status AS ENUM (
    'created', 'attempted', 'paid', 'expired'
);

CREATE TYPE entry_type AS ENUM ('debit', 'credit');



CREATE TABLE merchants (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    business_name       TEXT NOT NULL,
    email               TEXT NOT NULL UNIQUE,
    phone               TEXT,
    website             TEXT,
    kyc_status          kyc_status NOT NULL DEFAULT 'pending',

    
    api_key_hash        TEXT NOT NULL UNIQUE,
    test_api_key_hash   TEXT NOT NULL UNIQUE,

    
    webhook_url         TEXT,
    webhook_secret_enc  TEXT,   

    
    fee_plan_id         UUID,

    is_active           BOOLEAN NOT NULL DEFAULT true,

    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER merchants_updated_at
    BEFORE UPDATE ON merchants
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE INDEX idx_merchants_email ON merchants(email);
CREATE INDEX idx_merchants_kyc_status ON merchants(kyc_status);



CREATE TABLE fee_plans (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                TEXT NOT NULL,
    
    percentage_fee_bps  INTEGER NOT NULL DEFAULT 200,
    
    fixed_fee           INTEGER NOT NULL DEFAULT 0,
    fixed_fee_currency  currency_code NOT NULL DEFAULT 'INR',
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


INSERT INTO fee_plans (id, name, percentage_fee_bps, fixed_fee)
VALUES (
    '00000000-0000-0000-0000-000000000001',
    'Standard',
    200,   
    0
);
