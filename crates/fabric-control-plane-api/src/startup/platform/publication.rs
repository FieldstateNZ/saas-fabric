//! Building this deployment's publication target, when it states one.

use std::sync::Arc;

use fabric_control_plane::PublicationSink;
use fabric_publication_kubernetes::{KubernetesRuntimePublication, PublicationTarget};
use fabric_runtime_publication::RuntimePublication;

use crate::config::PublicationConfig;

/// Builds the Kubernetes publication target, if this deployment states one.
///
/// Called from [`super::establish`] beside the budget check: a deployment
/// that states `[platform_management.publication]` wrongly must fail loudly
/// at startup, the same as a wrongly stated `registry`, not discover it on
/// its first scheduled pass.
///
/// # Errors
///
/// A message when the namespace is not a DNS label, or the cluster trust
/// root cannot be read -- `KubernetesRuntimePublication::in_cluster`'s own
/// two failures. Never a credential.
pub(super) fn build_sink(config: Option<&PublicationConfig>) -> Result<Option<PublicationSink>, String> {
    let Some(config) = config else {
        return Ok(None);
    };

    let target = KubernetesRuntimePublication::in_cluster(PublicationTarget {
        namespace: config.namespace.clone(),
    })?;

    Ok(Some(PublicationSink {
        target: Arc::new(target) as Arc<dyn RuntimePublication>,
    }))
}
