CREATE TABLE orders (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id     UUID NOT NULL REFERENCES merchants(id) ON DELETE RESTRICT,

    amount          BIGINT NOT NULL CHECK (amount > 0),
    currency        currency_code NOT NULL,

    status          order_status NOT NULL DEFAULT 'created',

    customer_id     TEXT,
    customer_email  TEXT,
    description     TEXT,
    metadata        JSONB DEFAULT '{}',

    
    payment_id      UUID REFERENCES payments(id),

    
    expires_at      TIMESTAMPTZ NOT NULL DEFAULT (NOW() + INTERVAL '15 minutes'),

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TRIGGER orders_updated_at
    BEFORE UPDATE ON orders
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE INDEX idx_orders_merchant_id ON orders(merchant_id);
CREATE INDEX idx_orders_status ON orders(status);
CREATE INDEX idx_orders_expires_at ON orders(expires_at) WHERE status = 'created';


ALTER TABLE payments ADD CONSTRAINT fk_payments_order_id
    FOREIGN KEY (order_id) REFERENCES orders(id);
