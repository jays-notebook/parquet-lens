//! Query engine layer — owns the DataFusion `SessionContext`.
//!
//! Re-exports `QueryEngine` from `context.rs` for use by the command layer.
//! `executor.rs` extends `QueryEngine` with `execute` and `get_page` methods (Plan 02).

pub mod context;
pub mod executor;

pub use context::QueryEngine;
