//! Walking the chain from a tenant to something executable.

#[cfg(test)]
mod placement_inertness_tests;
mod resolved_data_source;
mod runtime_resolver;
#[cfg(test)]
mod runtime_resolver_tests;

pub use resolved_data_source::ResolvedDataSource;
pub use runtime_resolver::RuntimeResolver;
