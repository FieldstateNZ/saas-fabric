//! Where the runtime's three documents are published, when this deployment
//! publishes at all.

use std::sync::Arc;

use fabric_runtime_publication::{IssuerOrigin, RuntimePublication};

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
    /// The identity provider's public origin, under which each client's
    /// realm is its issuer. `None` publishes no issuer registry (D01-4).
    pub issuer_origin: Option<IssuerOrigin>,
    /// Samples which generation the runtime's replicas serve (G7). `None`
    /// observes nothing, and every reader answer is unobserved.
    pub readers: Option<Arc<dyn fabric_platform_management::ReaderObserver>>,
}
