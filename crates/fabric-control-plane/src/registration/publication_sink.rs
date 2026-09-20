//! Where the runtime's three documents are published, when this deployment
//! publishes at all.

use std::sync::Arc;

use fabric_runtime_publication::RuntimePublication;

/// The publication target this deployment writes through.
///
/// Named apart from `fabric_publication_kubernetes::PublicationTarget` --
/// that names a namespace, a piece of configuration; this names the port a
/// [`crate::PlatformBinding::publisher`] writes through, whatever backs it
/// (a filesystem in tests, `KubernetesRuntimePublication` in production).
/// `Option<PublicationSink>` on [`crate::ControlPlaneDeps`] is this
/// deployment's whole answer to "does it publish runtime state at all" --
/// absent for every deployment that does not state
/// `[platform_management.publication]`.
pub struct PublicationSink {
    /// Where publication writes.
    pub target: Arc<dyn RuntimePublication>,
}
