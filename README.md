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

*(Add instructions on how to run locally, typically `docker-compose up` and `cargo run`)*
