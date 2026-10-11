//! Building the runtime publisher for one platform binding.

use std::sync::Arc;

use fabric_core::Clock;
use fabric_platform_management::{PlatformRepository, RuntimeCatalogueSource, RuntimePublisher};

use super::{PlatformBinding, PublicationSink};
use crate::repository::DesiredStateBinding;
use crate::service::{ClientService, DesiredStateCatalogueSource, LifecycleIssuerRegistry};

/// The publisher for `binding`'s environment, writing through `sink`: the
/// catalogue read through the client desired-state binding, and -- when the
/// sink names an issuer origin -- the issuer registry decided by each
/// client's lifecycle (D01-4).
pub(super) fn build(
    binding: &PlatformBinding,
    sink: &PublicationSink,
    repository: &Arc<DesiredStateBinding>,
    service: &Arc<ClientService>,
    clock: &Arc<dyn Clock>,
) -> Arc<RuntimePublisher> {
    let catalogue: Arc<dyn RuntimeCatalogueSource> =
        Arc::new(DesiredStateCatalogueSource::new(Arc::clone(repository)));
    let publisher = RuntimePublisher::new(
        binding.environment.clone(),
        Arc::clone(&binding.repository) as Arc<dyn PlatformRepository>,
        catalogue,
        Arc::clone(&sink.target),
        Arc::clone(clock),
    );
    Arc::new(match &sink.issuer_origin {
        Some(origin) => publisher.with_issuer_registry(Arc::new(LifecycleIssuerRegistry::new(
            Arc::clone(service),
            origin.clone(),
        ))),
        None => publisher,
    })
}
