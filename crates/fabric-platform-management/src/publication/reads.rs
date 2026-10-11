//! Reading a pass's three inputs, and turning each failure into a
//! [`Halt`]: a coherence problem this platform itself wrote, a catalogue
//! conflict, or a store that refused the request outright is a refusal; an
//! unconnected platform repository is a wait, not a failure; anything else
//! reaching either store is a transport failure.

#[cfg(test)]
#[path = "reads_tests.rs"]
mod reads_tests;

use fabric_runtime_publication::CatalogDocument;

use crate::data_sources::held::check_held;
use crate::placements::held::check_held_placements;
use crate::publication::catalogue_source::CatalogueSourceError;
use crate::publication::outcome::WaitingReason;
use crate::publication::pass::Halt;
use crate::publication::RuntimePublisher;
use crate::{DataSourceDeclaration, DesiredStateError, PlacementRecord, PlatformError, SafeDiagnostic};

/// Sorts a desired-state read failure the same way `DesiredStateError`
/// itself distinguishes them: [`DesiredStateError::Refused`] is a store
/// that understood the request and said no -- a malformed held document, a
/// wrong `environment:` header, or a refused credential -- which is a
/// human's problem, the same as a coherence problem this platform wrote
/// itself. [`DesiredStateError::NotConnected`] is not a failure at all.
/// [`DesiredStateError::Unavailable`] and [`DesiredStateError::Conflict`]
/// -- and [`DesiredStateError::NotFound`], unreachable through these two
/// reads in practice -- are transport problems a retry can fix.
fn classify(error: &DesiredStateError) -> Halt {
    match error {
        DesiredStateError::Refused { .. } => Halt::Refused(SafeDiagnostic::sanitise(&error.to_string())),
        DesiredStateError::NotConnected => Halt::Waiting(WaitingReason::PlatformNotConnected),
        DesiredStateError::Unavailable { .. }
        | DesiredStateError::Conflict
        | DesiredStateError::NotFound { .. } => Halt::Failed(SafeDiagnostic::sanitise(&error.to_string())),
    }
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
            .map_err(|error| classify(&error))?;
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
            .map_err(|error| classify(&error))?;
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
