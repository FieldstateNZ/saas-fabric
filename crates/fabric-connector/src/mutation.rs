//! Write operations across the boundary.

mod mutation_outcome;
mod mutation_spec;
#[cfg(test)]
mod mutation_spec_tests;

pub use mutation_outcome::MutationOutcome;
pub use mutation_spec::MutationSpec;
