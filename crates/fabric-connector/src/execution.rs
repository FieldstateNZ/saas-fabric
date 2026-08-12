//! Where a tenant's data physically lives, and how it is kept apart from other
//! tenants' data.

mod connection_selector;
mod execution_target;
mod isolation_model;

pub use connection_selector::ConnectionSelector;
pub use execution_target::ExecutionTarget;
pub use isolation_model::IsolationModel;
