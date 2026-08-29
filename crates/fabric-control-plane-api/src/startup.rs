//! Assembling the control plane.

mod adapters;
mod application;
mod health;
mod local_documents;
mod operator_keys;
mod serving;
mod shutdown;

pub use application::{build, Application};
pub use shutdown::shutdown_signal;
