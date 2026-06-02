use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;


fn db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .expect("TEST_DATABASE_URL or DATABASE_URL must be set for invariant tests")
}

async fn connect() -> PgPool {
    PgPool::connect(&db_url())
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn invariant_ledger_balanced_per_merchant() {
    let db = connect().await;

    let merchants: Vec<Uuid> = sqlx::query_scalar("SELECT DISTINCT merchant_id FROM ledger_entries")
    .fetch_all(&db)
    .await
    .expect("Failed to query ledger merchants");

    for merchant_id in merchants {
        let row = sqlx::query(
            r#"
            SELECT
                SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE 0 END)::BIGINT AS total_credits,
                SUM(CASE WHEN entry_type = 'debit'  THEN amount ELSE 0 END)::BIGINT AS total_debits
            FROM ledger_entries
            WHERE merchant_id = $1
            "#,
        )
        .bind(merchant_id)
        .fetch_one(&db)
        .await
        .expect("Ledger balance query failed");

        let credits = row
            .try_get::<Option<i64>, _>("total_credits")
            .unwrap_or_else(|_| panic!("Failed to decode total_credits for merchant {merchant_id}"))
            .unwrap_or(0);
        let debits = row
            .try_get::<Option<i64>, _>("total_debits")
            .unwrap_or_else(|_| panic!("Failed to decode total_debits for merchant {merchant_id}"))
            .unwrap_or(0);

        
        assert_eq!(
            credits, debits,
            "INVARIANT VIOLATION: Ledger imbalance for merchant {merchant_id}. \
             credits={credits}, debits={debits}, diff={}",
            credits - debits
        );
    }
}










#[tokio::test]
async fn invariant_refund_not_exceed_captured() {
    let db = connect().await;

    let violations: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT
            p.id AS payment_id
        FROM payments p
        JOIN refunds r ON r.payment_id = p.id
        WHERE r.status = 'succeeded'
        GROUP BY p.id, p.captured_amount, p.amount
        HAVING SUM(r.amount) > COALESCE(p.captured_amount, p.amount)
        "#,
    )
    .fetch_all(&db)
    .await
    .expect("Refund invariant query failed");

    assert!(
        violations.is_empty(),
        "INVARIANT VIOLATION: {} payment(s) have total refunds exceeding captured amount: {:?}",
        violations.len(),
        violations
    );
}






#[tokio::test]
async fn invariant_refunded_amount_matches_refund_sum() {
    let db = connect().await;

    let violations: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT
            p.id AS payment_id
        FROM payments p
        LEFT JOIN refunds r ON r.payment_id = p.id AND r.status = 'succeeded'
        GROUP BY p.id, p.refunded_amount
        HAVING p.refunded_amount != COALESCE(SUM(r.amount), 0)
        "#,
    )
    .fetch_all(&db)
    .await
    .expect("Refund sum invariant query failed");

    assert!(
        violations.is_empty(),
        "INVARIANT VIOLATION: {} payment(s) have mismatched refunded_amount column: {:?}",
        violations.len(),
        violations
    );
}






#[tokio::test]
async fn invariant_ledger_entries_are_paired() {
    let db = connect().await;

    
    let orphaned: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM ledger_entries le
        WHERE le.reference_entry_id IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM ledger_entries ref
              WHERE ref.id = le.reference_entry_id
          )
        "#,
    )
    .fetch_one(&db)
    .await
    .expect("Pair check query failed");

    assert_eq!(
        orphaned, 0,
        "INVARIANT VIOLATION: {orphaned} ledger entries have orphaned reference_entry_id"
    );
}






#[tokio::test]
async fn invariant_outbox_events_eventually_published() {
    let db = connect().await;

    
    let stuck: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM outbox_events
        WHERE published = false
          AND failed_attempts < 10
          AND created_at < NOW() - INTERVAL '5 minutes'
        "#,
    )
    .fetch_one(&db)
    .await
    .expect("Outbox stuck check failed");

    assert_eq!(
        stuck, 0,
        "INVARIANT VIOLATION: {stuck} outbox events are more than 5 minutes old and unpublished. \
         Check the outbox-worker process."
    );
}
