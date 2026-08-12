//! Assembling the application.
//!
//! The composition root, split so that each thing the process trusts can be
//! read on its own: the identity posture, the connectors, the catalogue, and
//! the wiring that joins them.
//!
//! There is no dependency-injection container, no assembly scanning, and no
//! registry macros anywhere in here. In a system whose whole job is keeping
//! tenants apart, "what does this process actually trust?" should be answerable
//! by reading a handful of short functions rather than by tracing attributes
//! through six crates.

mod application;
mod catalog;
mod connectors;
mod shutdown;
mod token_reader;

pub use application::{build, Application};
pub use connectors::ConnectorRetryHandle;
pub use shutdown::shutdown_signal;
