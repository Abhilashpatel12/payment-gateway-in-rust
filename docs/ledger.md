# Double-Entry Immutable Ledger

The Rust Payment Gateway implements a strict, financial-grade double-entry ledger to guarantee absolute consistency of funds. 

## Core Principles
1. **Double-Entry Accounting**: Every financial movement creates two paired records: a `debit` (money leaving an account) and a `credit` (money entering an account).
2. **Immutability**: Once a ledger entry is written, it can **never** be updated or deleted. This is enforced at the database level using PostgreSQL rules.
3. **Cryptographic Hashing**: Each entry is hashed to detect any tampering or manual database manipulation.

## Database Schema

```sql
CREATE TABLE ledger_entries (
    id              UUID PRIMARY KEY,
    payment_id      UUID NOT NULL REFERENCES payments(id),
    merchant_id     UUID NOT NULL REFERENCES merchants(id),
    entry_type      TEXT NOT NULL CHECK (entry_type IN ('debit', 'credit')),
    amount          BIGINT NOT NULL CHECK (amount > 0),
    currency        TEXT NOT NULL,
    balance_before  BIGINT NOT NULL,
    balance_after   BIGINT NOT NULL,
    account_id      UUID REFERENCES ledger_accounts(id),
    reference_entry_id UUID REFERENCES ledger_entries(id) DEFERRABLE INITIALLY DEFERRED,
    description     TEXT NOT NULL,
    hash            TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Aggressive immutability trigger
CREATE RULE ledger_no_update AS ON UPDATE TO ledger_entries DO INSTEAD NOTHING;
CREATE RULE ledger_no_delete AS ON DELETE TO ledger_entries DO INSTEAD NOTHING;
```

## Standard Flows

### 1. Payment Capture
When a customer's card is successfully charged:
- **Debit**: `merchant_payable` (Liability account representing funds owed to the merchant)
- **Credit**: `gateway_float` (Asset account representing funds currently held by our gateway)

### 2. Refunds
When a merchant issues a refund:
- **Debit**: `gateway_float` (Funds leaving the gateway)
- **Credit**: `merchant_payable` (Reducing the amount we owe the merchant)

### 3. Fee Collection
When the platform takes a cut (e.g., 2% transaction fee):
- **Debit**: `merchant_payable` (Reducing what we owe the merchant)
- **Credit**: `fee_income` (Platform Revenue account)

## Security and Compliance
By using PostgreSQL's `DEFERRABLE INITIALLY DEFERRED` foreign key constraints, we ensure the paired `debit` and `credit` rows must exist simultaneously by the time the transaction commits, effectively eliminating circular dependency issues while guaranteeing the ledger balances.
