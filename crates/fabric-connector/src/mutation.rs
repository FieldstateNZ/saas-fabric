//! Write operations across the boundary.

mod mutation_outcome;
mod mutation_spec;
#[cfg(test)]
mod mutation_spec_negation_hostility_tests;
#[cfg(test)]
mod mutation_spec_nesting_hostility_tests;
#[cfg(test)]
mod mutation_spec_stamp_hostility_tests;
#[cfg(test)]
mod mutation_spec_tests;
#[cfg(test)]
mod mutation_spec_widening_hostility_tests;

pub use mutation_outcome::MutationOutcome;
pub use mutation_spec::MutationSpec;
