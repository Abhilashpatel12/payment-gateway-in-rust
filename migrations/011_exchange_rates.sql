DROP TABLE IF EXISTS exchange_rates CASCADE;

CREATE TABLE exchange_rates (
    base_currency currency_code NOT NULL,
    target_currency currency_code NOT NULL,
    rate DECIMAL(15, 6) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (base_currency, target_currency)
);

CREATE INDEX idx_exchange_rates_updated_at ON exchange_rates(updated_at);
