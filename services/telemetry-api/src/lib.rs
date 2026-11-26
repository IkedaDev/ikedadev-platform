//! # IkedaDev Telemetry API
//!
//! Multi-tenant, append-only telemetry (`TRACKING` & `AUDIT`) microservice with
//! PostgreSQL Row-Level Security (RLS), high-throughput `UNNEST` batch ingestion,
//! dynamic `Criteria` + JSONB search, client timelines, and aggregated metrics.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
