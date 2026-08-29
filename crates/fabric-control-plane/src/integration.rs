//! What the platform can say about its connection to client desired state.
//!
//! # Derived, never advanced
//!
//! There is no stored "we are connected now" flag that something has to
//! remember to set, and no state machine to get out of step with reality.
//! Status is computed from two facts the platform already has: whether a
//! repository is bound, and what the last reconciliation sweep observed when
//! it read one.
//!
//! That matters because the alternative fails quietly. A stored flag that says
//! `connected` while every read is refused is worse than no status at all — it
//! is a screen telling an operator nothing is wrong.

mod health;
mod status;
#[cfg(test)]
mod status_tests;

pub use health::{IntegrationHealth, Observation};
pub use status::IntegrationStatus;
