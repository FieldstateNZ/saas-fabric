//! Read operations across the boundary.

mod query_outcome;
mod query_spec;
#[cfg(test)]
mod query_spec_tests;

pub use query_outcome::QueryOutcome;
pub use query_spec::QuerySpec;
