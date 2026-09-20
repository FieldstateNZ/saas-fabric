//! Reading a pass's three inputs, and turning each failure into a
//! [`Halt`] -- a coherence problem this platform itself wrote, or a
//! catalogue conflict, versus a transport failure reaching either.

use fabric_runtime_publication::CatalogDocument;

use crate::data_sources::held::check_held;
use crate::placements::held::check_held_placements;
use crate::publication::catalogue_source::CatalogueSourceError;
use crate::publication::pass::Halt;
use crate::publication::RuntimePublisher;
use crate::{DataSourceDeclaration, DesiredStateError, PlacementRecord, PlatformError, SafeDiagnostic};

fn transport_failure(error: &DesiredStateError) -> Halt {
    Halt::Failed(SafeDiagnostic::sanitise(&error.to_string()))
}

/// `check_held` and `check_held_placements` only ever answer their own
/// coherence variant, so every error either can produce is a refusal, not
/// a transport failure.
fn coherence_refusal(error: &PlatformError) -> Halt {
    Halt::Refused(SafeDiagnostic::sanitise(&error.to_string()))
}

impl RuntimePublisher {
    pub(super) async fn declared_data_sources(&self) -> Result<Vec<DataSourceDeclaration>, Halt> {
        let read = self
            .platform
            .read_data_sources(&self.environment)
            .await
            .map_err(|error| transport_failure(&error))?;
        check_held(&read.declarations).map_err(|error| coherence_refusal(&error))?;
        Ok(read.declarations)
    }

    pub(super) async fn recorded_placements(
        &self,
        declared: &[DataSourceDeclaration],
    ) -> Result<Vec<PlacementRecord>, Halt> {
        let read = self
            .platform
            .read_placements(&self.environment)
            .await
            .map_err(|error| transport_failure(&error))?;
        check_held_placements(&read.placements, declared).map_err(|error| coherence_refusal(&error))?;
        Ok(read.placements)
    }

    pub(super) async fn runtime_catalogue(&self) -> Result<CatalogDocument, Halt> {
        self.catalogue
            .runtime_catalogue()
            .await
            .map_err(|error| match error {
                CatalogueSourceError::Conflict { .. } => {
                    Halt::Refused(SafeDiagnostic::sanitise(&error.to_string()))
                }
                CatalogueSourceError::Unavailable(detail) => Halt::Failed(SafeDiagnostic::sanitise(&detail)),
            })
    }
}
