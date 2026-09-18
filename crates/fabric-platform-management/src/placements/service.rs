//! Placing a client's data intent, and previewing what placing it would do.

mod for_client;
#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;
mod stamp;

use std::sync::Arc;

use fabric_core::{Clock, LogicalDataSourceName};

use crate::data_sources::held::check_held;
use crate::placements::held::check_held_placements;
use crate::placements::intent::DataIntent;
use crate::placements::read::PlacementsRead;
use crate::placements::select::select;
use crate::placements::tenant_id::tenant_id;
use crate::{DesiredRevision, DesiredStateError, EnvironmentWrite, PlatformError, PlatformRepository};

/// Placing a client's data intent, over whatever holds data sources and
/// placements.
pub struct Placements {
    /// Every port this service reads and writes through, including
    /// [`write_environment`](PlatformRepository::write_environment) --
    /// `place` needs every one of them at once, to write both documents in
    /// the one atomic commit ADR 0023 part 2 (B4) requires.
    repository: Arc<dyn PlatformRepository>,

    /// Stamps a new record with when it was placed.
    clock: Arc<dyn Clock>,
}

impl Placements {
    /// Builds the service over its repository and a clock.
    #[must_use]
    pub fn new(repository: Arc<dyn PlatformRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }

    /// Places one logical data source's intent and records the outcome.
    ///
    /// `client` is the client id exactly as the caller holds it -- this
    /// reparses it as a `TenantId`, the same identifier the placement is
    /// recorded under, so the control plane never has to (ADR 0023 part 2,
    /// N11): the record this produces is what the runtime reads, and the
    /// conversion belongs beside the write that depends on it having
    /// happened.
    ///
    /// `at` must equal the held placements revision, checked immediately
    /// after that read and before data sources are even read, let alone
    /// validated -- the same precondition-before-planning order
    /// `DataSources::declare` uses, and for the same reason: a stale `at`
    /// must be refused whether or not the placement it named would have
    /// changed anything.
    ///
    /// # Errors
    ///
    /// [`PlatformError::PlacementRefused`] if the client id is not a valid
    /// tenant id or `select` refuses the intent,
    /// [`DesiredStateError::Conflict`] if `at` does not match what is
    /// held, or if a data source declared between this call's own reads
    /// moved before the write landed, and the `InvalidHeld*` variants if
    /// either held document is no longer coherent.
    pub async fn place(
        &self,
        environment: &str,
        client: &str,
        logical: &LogicalDataSourceName,
        intent: &DataIntent,
        at: Option<&DesiredRevision>,
    ) -> Result<PlacementsRead, PlatformError> {
        let tenant = tenant_id(client)?;

        let held = self.repository.read_placements(environment).await?;
        if at != held.revision.as_ref() {
            return Err(DesiredStateError::Conflict.into());
        }

        let declared = self.repository.read_data_sources(environment).await?;
        check_held(&declared.declarations)?;
        check_held_placements(&held.placements, &declared.declarations)?;

        let now = self.stamp()?;
        let record = select(
            intent,
            &tenant,
            logical,
            &declared.declarations,
            &held.placements,
            &now,
        )?;

        let mut placements = held.placements.clone();
        placements.push(record.clone());
        placements.sort_by(|left, right| (&left.tenant, &left.logical).cmp(&(&right.tenant, &right.logical)));

        let message = format!(
            "Place {tenant} {logical} on {} in {environment}",
            record.data_source
        );

        self.repository
            .write_environment(
                environment,
                EnvironmentWrite {
                    data_sources: (&declared.declarations, declared.revision.as_ref()),
                    placements: (&placements, at),
                },
                &message,
            )
            .await?;

        Ok(self.repository.read_placements(environment).await?)
    }

    /// The repository every method on this service reads and writes
    /// through -- borrowed by [`for_client`](Self::for_client), split into
    /// its own file. Private visibility already reaches it: `for_client`
    /// is a descendant of this module.
    fn repository(&self) -> &dyn PlatformRepository {
        self.repository.as_ref()
    }
}
