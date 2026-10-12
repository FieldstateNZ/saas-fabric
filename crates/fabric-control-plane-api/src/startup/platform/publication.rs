//! Building this deployment's publication target, when it states one.

use std::sync::Arc;

use fabric_control_plane::PublicationSink;
use fabric_core::Clock;
use fabric_deployment_kubernetes::KubernetesReaderObserver;
use fabric_platform_management::ReaderObserver;
use fabric_publication_kubernetes::{KubernetesRuntimePublication, PublicationTarget};
use fabric_runtime_publication::{IssuerOrigin, RuntimePublication};

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
/// A message when the issuer origin is not `https://<host>[:<port>]`, the
/// namespace is not a DNS label, a stated reader target is invalid, or the
/// cluster trust root cannot be read. Never a credential.
pub(super) fn build_sink(
    config: Option<&PublicationConfig>,
    clock: &Arc<dyn Clock>,
) -> Result<Option<PublicationSink>, String> {
    let Some(config) = config else {
        return Ok(None);
    };

    let issuer_origin = config
        .issuer_origin
        .as_deref()
        .map(IssuerOrigin::parse)
        .transpose()
        .map_err(|error| format!("[platform_management.publication].issuer_origin: {error}"))?;
    let target = KubernetesRuntimePublication::in_cluster(PublicationTarget {
        namespace: config.namespace.clone(),
    })?;

    let readers = config
        .readers
        .clone()
        .map(|target| KubernetesReaderObserver::in_cluster(target, Arc::clone(clock)))
        .transpose()
        .map_err(|error| format!("[platform_management.publication.readers]: {error}"))?
        .map(|observer| Arc::new(observer) as Arc<dyn ReaderObserver>);

    Ok(Some(PublicationSink {
        target: Arc::new(target) as Arc<dyn RuntimePublication>,
        issuer_origin,
        readers,
    }))
}
