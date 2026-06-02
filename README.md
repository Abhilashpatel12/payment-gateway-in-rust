# Rust Payment Gateway

A high-throughput, horizontally scalable microservices payment gateway built in Rust. It guarantees zero duplicate charges, perfect ledger accounting, and sub-second p99 latencies even under extreme load.

## Overview

The Rust Payment Gateway is designed to be a robust, financial-grade system handling payments, ledgers, fraud detection, and reconciliations. It leverages the following technologies to achieve hyper-scale and reliability:

- **Rust (Axum + Tokio)**: For lightning-fast, concurrent API serving and microservices.
- **PostgreSQL**: Serving as the source of truth for ACID transactions, the immutable double-entry ledger, and the transactional outbox.
- **Redis**: For high-speed rate limiting and distributed idempotency locking.
- **Apache Kafka**: For resilient, asynchronous event streaming between microservices.

## Project Structure

This project is organized as a Cargo workspace with multiple independent microservices:
- **API Gateway**: Single entry point handling auth and rate-limiting.
- **Payment Service**: The core state machine routing charges.
- **Vault Service**: For PCI-compliant tokenization.
- **Fraud Service**: Risk engine evaluation.
- **Ledger Service**: Immutable double-entry accounting.
- **Workers**: Asynchronous workers for Webhooks, Settlements, and Outbox event processing.
- **Adapters**: Connectors for external providers like Stripe or UPI.

## Documentation Reference

The `docs/` folder contains detailed technical explanations of the core architectural patterns used in this system.

- **[System Architecture](docs/architecture.md)**
  Provides a high-level overview of the microservices, ingress routing, asynchronous workers, and data persistence layers with sequence diagrams.

- **[Idempotency Engine](docs/idempotency.md)**
  Explains how the gateway safely handles network failures and client retries using an `X-Idempotency-Key` to prevent duplicate charges and cache successful responses.

- **[Double-Entry Immutable Ledger](docs/ledger.md)**
  Details the financial-grade ledger design where every movement creates a paired debit and credit, enforced by strict immutability rules at the PostgreSQL level.

- **[Transactional Outbox Pattern](docs/outbox.md)**
  Describes the mechanism used to guarantee that database state changes (like capturing a payment) and event publishing (like webhooks to Kafka) never fall out of sync.

- **[Reconciliation Service](docs/reconciliation.md)**
  Outlines the automated process of downloading end-of-day settlement reports from external acquirers and matching them against internal ledgers to detect missing revenue or phantom charges.

- **[Scalability Analysis & TPS Estimation](docs/scalability_analysis.md)**
  Contains theoretical throughput limits for each system component (Axum, Redis, Kafka, Postgres) along with empirical load test benchmark results proving the system's structural integrity.

## Getting Started

To run the RustPay gateway locally, you'll need Docker and Cargo installed.

1. **Start infrastructure dependencies:**
   Spin up PostgreSQL, Redis, Kafka, Zookeeper, Prometheus, Grafana, and Jaeger:
   ```bash
   docker-compose up -d
   ```

2. **Configure Environment:**
   Copy the example environment file and set your configurations:
   ```bash
   cp .env.example .env
   ```

3. **Run Migrations:**
   The database migrations are applied automatically when the `postgres` container starts up via the `./migrations` volume. Wait a few seconds for the database to be ready.

4. **Start the Microservices:**
   Because this is a workspace with multiple independent microservices, Cargo needs to know which one you want to run. It's recommended to open multiple terminal tabs and run the core services individually using the `--bin` flag:

   **Terminal 1 (Gateway):**
   ```bash
   cargo run --bin api-gateway
   ```

   **Terminal 2 (Core Payments):**
   ```bash
   cargo run --bin payment-service
   ```

   **Terminal 3 (Ledger):**
   ```bash
   cargo run --bin ledger-service
   ```

   **Terminal 4 (Outbox Worker for Kafka):**
   ```bash
   cargo run --bin outbox-worker
   ```

   *(Note: You can optionally use a process manager like `overmind` or `honcho` to start them all at once).*
