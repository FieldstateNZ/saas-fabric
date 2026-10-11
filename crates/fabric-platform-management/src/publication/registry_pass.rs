//! Deciding the issuer registry a pass offers, from the generation it is
//! about to offer.

use fabric_runtime_publication::RuntimeSnapshot;

use super::issuer_registry_source::{RegistryEvidence, RegistrySourceError, Withheld};
use super::pass::Halt;
use super::RuntimePublisher;
use crate::{PlacementRecord, SafeDiagnostic};

impl RuntimePublisher {
    /// Sets `snapshot`'s registry to what the source registers against its
    /// own tenants document, and returns the bound tenants it withheld.
    /// Without a source the snapshot carries no registry.
    ///
    /// A source that cannot answer halts the pass as `Failed`: offering the
    /// documents without the registry would be refused once one is
    /// published, and offering a registry missing clients would withdraw
    /// them from the edge because of a read failure.
    pub(super) async fn decide_registry(
        &self,
        snapshot: &mut RuntimeSnapshot,
        placements: &[PlacementRecord],
    ) -> Result<Vec<Withheld>, Halt> {
        let Some(source) = &self.registry else {
            return Ok(Vec::new());
        };
        let evidence = RegistryEvidence {
            placements,
            tenants: &snapshot.tenants.payload,
        };
        let registrations =
            source
                .registrations(&evidence)
                .await
                .map_err(|RegistrySourceError::Unavailable(detail)| {
                    Halt::Failed(SafeDiagnostic::sanitise(&detail))
                })?;
        snapshot.registry = Some(registrations.registered);
        Ok(registrations.withheld)
    }
}
