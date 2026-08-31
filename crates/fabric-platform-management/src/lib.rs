//! Deciding which version of a component an environment should run.
//!
//! ```text
//! Available     discovered from artifact registries   ← this crate asks
//! Desired       the platform repository                ← this crate proposes
//! Running       the reconciliation system              ← not yet answered
//! ```
//!
//! Three states, and they are deliberately not one. A version that has been
//! published is not one that has been chosen, and a version that has been
//! chosen is not one that is serving traffic. Collapsing any pair of them
//! produces a console that reports success from a Git commit.
//!
//! # No transport
//!
//! This crate knows nothing about HTTP, registries or Git. It defines the
//! [`Registry`] port and is handed an implementation; where an artifact was
//! found and how it was authenticated to is somebody else's concern, and
//! deliberately so — the registry credential and the platform repository
//! credential are separate integrations and must stay separable.

mod desired_state;
mod discovery;
mod policy;
mod registry;
mod selector;
mod service;
mod status;
mod version;

pub use desired_state::{ComponentDesired, DesiredState, DesiredStateError, Hold};
pub use discovery::{Discovery, ReleaseUnit, ResolvedImage};
pub use policy::UpdatePolicy;
pub use registry::{Provenance, Registry, RegistryError, Resolved};
pub use selector::{decide, Decision, Reason};
pub use service::{PlatformError, PlatformManagement};
pub use status::{ComponentStatus, DesiredStateStatus, Diagnostics, Running};
pub use version::{Channel, Version};

/// Finds the newest release unit an environment is allowed to move to.
pub use discovery::discover;
