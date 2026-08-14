//! Where a tenant's data physically lives, and how it is kept apart from other
//! tenants' data.

mod connection_selector;
mod execution_target;
#[cfg(test)]
mod execution_target_tests;
mod isolation_model;
mod tagged_documents;

pub use connection_selector::ConnectionSelector;
pub use execution_target::ExecutionTarget;
pub use isolation_model::IsolationModel;
