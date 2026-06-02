# Reconciliation Service

Even with perfect internal ledgers, a payment gateway must continuously verify its records against its external processing partners (Acquirers, Stripe, Adyen) to ensure that the physical movement of funds matches the expected digital ledger.

## Mechanism

1. **Daily Report Ingestion**: The Reconciliation Service automatically downloads end-of-day settlement reports from external providers via SFTP or API.
2. **Matching Engine**: It correlates external transactions to internal `payment_id`s.
3. **Discrepancy Flags**: It flags mismatches where:
    - We captured a payment, but the provider didn't (Lost Revenue).
    - The provider captured a payment, but we didn't (Phantom Charge).
    - The fee charged by the provider doesn't match our `fee_income` ledger (Margin Leakage).

## Database Schema

```sql
CREATE TABLE reconciliation_runs (
    id                  UUID PRIMARY KEY,
    provider            TEXT NOT NULL,    
    period_start        TIMESTAMPTZ NOT NULL,
    period_end          TIMESTAMPTZ NOT NULL,
    status              TEXT NOT NULL DEFAULT 'running',  
    payments_checked    INTEGER NOT NULL DEFAULT 0,
    mismatches_found    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE reconciliation_mismatches (
    id              UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES reconciliation_runs(id),
    payment_id      UUID REFERENCES payments(id),
    mismatch_type   TEXT NOT NULL,  
    our_value       TEXT,
    provider_value  TEXT,
    resolved_at     TIMESTAMPTZ
);
```

## Resolution Workflow
Any records landing in `reconciliation_mismatches` require manual intervention by the Finance/Ops team to either issue a manual ledger adjustment or dispute the charge with the external provider.
