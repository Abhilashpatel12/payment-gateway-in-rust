# Idempotency Engine

To safely handle network failures, timeouts, and aggressive client retries, the Payment Gateway requires an `X-Idempotency-Key` header on all mutative requests.

## Architecture

1. **Request Hashing**: The incoming payload is hashed alongside the `Idempotency-Key`.
2. **Database UPSERT**: 
   The system attempts to insert an idempotency record. If a record already exists for that key, it returns the existing record instead.
   
```sql
INSERT INTO idempotency_keys (key, idempotency_key, request_hash)
VALUES ($1, $2, $3)
ON CONFLICT (idempotency_key) DO UPDATE SET request_hash = EXCLUDED.request_hash
RETURNING id, response_status, response_body;
```

3. **Validation**: If the request hash doesn't match the one originally saved for that key, a `400 Bad Request` is thrown to prevent the client from sending different payloads with the same key.
4. **Response Caching**: If the previous request succeeded, the engine directly returns the cached `response_body` and skips the entire business logic and acquirer call, saving database compute and network bandwidth.

## Concurrency Race Conditions
During the load test (`concurrent_capture_race`), 50 Virtual Users try to capture the *exact same payment* concurrently. The idempotency engine utilizes row-level locking (`SELECT ... FOR UPDATE`) in Postgres to guarantee that exactly 1 request triggers the capture, while the other 49 get the gracefully cached response. 
