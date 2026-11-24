//! # IkedaDev Bookings API
//!
//! Multi-tenant, 100% domain-agnostic commercial bookings microservice with frozen
//! line-item snapshots, strict lifecycle state machine, transactional idempotency,
//! PostgreSQL Row-Level Security (RLS), and dynamic `Criteria` search.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
