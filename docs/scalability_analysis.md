# Scalability Analysis

## Overview

RustPay is designed as a distributed payment gateway that prioritizes financial correctness, reliability, and horizontal scalability.

The system is built using:

- Rust
- Axum
- Tokio
- PostgreSQL
- Redis
- Apache Kafka

Key architectural patterns include:

- Database-level idempotency
- Double-entry accounting
- Transactional Outbox Pattern
- Event-driven microservices
- Asynchronous webhook delivery
- Distributed workers
- Observability via Prometheus and OpenTelemetry

---

# Scalability Goals

RustPay is designed to scale horizontally by separating responsibilities across independent services and asynchronous processing pipelines.

Primary scalability objectives:

- Prevent duplicate charges under concurrent requests
- Maintain ledger consistency during failures
- Minimize synchronous request latency
- Decouple background processing from payment execution
- Support independent scaling of workers and services

---

# Architecture Scaling Characteristics

## API Layer (Axum + Tokio)

The API Gateway and Payment Service use Rust's asynchronous runtime to efficiently process concurrent requests.

### Characteristics

- Low runtime overhead
- Non-blocking I/O
- Horizontal scaling through service replication
- Efficient CPU and memory utilization

### Potential Bottlenecks

- Database connection availability
- External provider latency
- Cryptographic operations

---

## Redis

Redis is used for:

- Rate limiting
- Temporary coordination
- Request caching

### Characteristics

- In-memory operations
- Very low latency
- High throughput

### Potential Bottlenecks

- Memory limits
- Hot key contention
- Network saturation

### Scaling Strategy

- Redis Cluster
- Key partitioning
- Read replicas

---

## Kafka

Kafka is used as the event backbone for:

- Payment events
- Settlement workflows
- Reconciliation workflows
- Webhook delivery

### Characteristics

- Decouples services
- Supports horizontal consumer scaling
- Enables asynchronous processing

### Potential Bottlenecks

- Partition count
- Consumer lag
- Broker resource limits

### Scaling Strategy

- Additional partitions
- Additional brokers
- Independent consumer groups

---

## PostgreSQL

PostgreSQL is expected to be the primary scalability bottleneck.

Critical payment operations typically involve:

- Payment state updates
- Ledger entry inserts
- Outbox event inserts
- Transaction commits

### Potential Bottlenecks

- Connection pool saturation
- Lock contention
- Transaction duration
- Disk I/O limits

### Scaling Strategy

- PgBouncer
- Read replicas
- Table partitioning
- Merchant-based sharding

---

# Load Testing Results

## Test Environment

Infrastructure:

- macOS
- Docker
- PostgreSQL
- Redis
- Kafka
- RustPay Services
- k6 Load Generator

All services and the load generator were executed on a single machine.

---

## Verified Results

### Throughput

- ~194 requests/sec observed

### Latency

- Average: 8.02 ms
- p95: 54.59 ms
- p99: 152 ms

### Reliability

- Success Rate: 100%
- Duplicate Charges: 0
- Dropped Iterations: 0

---

## Key Findings

The system successfully maintained:

- Database-level idempotency
- Correct payment state transitions
- Stable latency under concurrent load
- Zero duplicate charges
- No observed application crashes

The primary objective of the benchmark was to validate correctness under concurrency rather than establish the maximum throughput of the architecture.

---

# Future Scaling Path

## Phase 1: Single-Node Deployment

Components:

- PostgreSQL
- Redis
- Kafka
- RustPay Services

Expected bottleneck:

- PostgreSQL write throughput

---

## Phase 2: Dedicated Infrastructure

Components:

- Separate database host
- Dedicated Kafka broker
- Dedicated Redis node

Benefits:

- Reduced resource contention
- Increased throughput
- Improved latency consistency

---

## Phase 3: Cloud Deployment

Infrastructure:

- Kubernetes / ECS
- Managed PostgreSQL
- Managed Kafka
- Managed Redis

Benefits:

- Horizontal service scaling
- Independent worker scaling
- Improved fault tolerance

---

# Future Improvements

Potential improvements include:

- PgBouncer connection pooling
- Debezium CDC for outbox streaming
- Kafka partition scaling
- Database partitioning
- Database sharding by merchant
- Read replicas for reporting workloads
- Dedicated load-testing infrastructure

---

# Conclusion

RustPay demonstrates a scalable distributed architecture built around financial correctness and asynchronous event processing.

Local benchmarking validates the architecture's behavior under concurrent load while maintaining:

- Zero duplicate charges
- Reliable state transitions
- Consistent ledger operations

Additional testing on distributed cloud infrastructure is required to determine the platform's true throughput limits.
