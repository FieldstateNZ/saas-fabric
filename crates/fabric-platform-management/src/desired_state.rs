//! The port through which an environment's desired state is read and moved.
//!
//! # Every operation must be bounded, and never by cancellation
//!
//! A contract, not a hope: the platform binding holds a lock across every
//! `DesiredState` call, so the longest one can take is the longest an
//! operator's disconnect can wait, and that is cut off by the API's request
//! timeout. Bounding each request separately is not enough -- an operation
//! is many of them -- so bound the *operation*, answering
//! [`Unavailable`](DesiredStateError::Unavailable) when the budget is spent.
//! Bound it by refusing to **start** a request it cannot afford, never by
//! abandoning a write already sent: one dropped mid-flight releases the
//! binding while it may still land, in a repository the platform has by
//! then reported it stopped writing to. So an operation ends within its
//! budget plus the one request it may still have running.
//!
//! Every write answers [`Conflict`](DesiredStateError::Conflict) if the
//! state it was decided against has moved since it was read, and the other
//! `DesiredStateError` variants for what they name -- said once here rather
//! than under each method on the port itself.

mod component;
mod errors;
mod port;

pub use component::{ComponentDesired, DesiredRevision, Hold};
pub use errors::DesiredStateError;
pub use port::DesiredState;
