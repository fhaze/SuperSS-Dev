//! Database layer: a sqlx connection pool and typed repositories.
//!
//! This replaces the C++ `pangya_db` / `Cmd*` command layer, which built every
//! `CALL proc(params)` query by string concatenation (a confirmed SQL-injection
//! surface). Every query here uses **bound parameters** — `makeText`-style value
//! quoting is gone entirely.
//!
//! The original relied on ~165 stored procedures as the source of truth, with
//! business logic split between procs and C++. Per the porting plan, that logic
//! is migrated into typed Rust repositories incrementally as each system is
//! touched. This module starts with the repos the Login flow (Milestone 1)
//! needs; more are added per phase.

pub mod pool;
pub mod repos;

pub use pool::{connect, DbPool};

/// Re-export the model types the repos return, so callers depend only on
/// `pangya-db` + `pangya-model` rather than reaching across crates.
pub use pangya_model::{Account, AuthKey, ServerEntry};
