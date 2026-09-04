//! The port through which an environment's desired state is read and moved.

mod component;
mod errors;
mod port;

pub use component::{ComponentDesired, DesiredRevision, Hold};
pub use errors::DesiredStateError;
pub use port::DesiredState;
