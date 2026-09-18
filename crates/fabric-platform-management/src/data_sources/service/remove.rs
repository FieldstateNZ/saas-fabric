//! Removing a declared data source, refusing while a tenant is placed on it.

use std::collections::BTreeSet;

use fabric_core::{DataSourceId, TenantId};

use crate::data_sources::declaration::DataSourceDeclaration;
use crate::data_sources::held::check_held;
use crate::data_sources::service::{DataSources, Declared};
use crate::placements::held::check_held_placements;
use crate::{DesiredRevision, DesiredStateError, EnvironmentWrite, PlatformError, PlatformRepository};

impl DataSources {
    /// Removes a declared data source, refusing while any tenant is
    /// placed on it.
    ///
    /// The file is never deleted -- removing the last declaration leaves
    /// an empty list, the same as an environment that has declared none.
    ///
    /// `repository` is taken per call rather than at construction, the
    /// same reason it always was: `list`/`declare` need only
    /// `DataSourceState`, and this is the one operation on this service
    /// that needs every port at once, to write both documents in the one
    /// atomic commit [`write_environment`](PlatformRepository::write_environment)
    /// makes (ADR 0023 part 2, B4) -- see that method's rustdoc for the
    /// race this closes: `place` reading data sources and writing
    /// placements while this reads placements and writes data sources
    /// could previously interleave and leave a placement naming a source
    /// nothing declares.
    ///
    /// # Errors
    ///
    /// [`PlatformError::DataSourceInUse`] naming every tenant still
    /// placed on it, [`DesiredStateError::Conflict`] if `at` does not
    /// match what is held (of *either* document -- the placements
    /// revision is this call's own read, and a placement recorded between
    /// that read and this write moves it too), and `InvalidHeldDataSources`/
    /// `InvalidHeldPlacements` if either held document is no longer
    /// coherent.
    pub async fn remove(
        &self,
        environment: &str,
        id: &DataSourceId,
        at: Option<&DesiredRevision>,
        repository: &dyn PlatformRepository,
    ) -> Result<Declared, PlatformError> {
        let held = repository.read_data_sources(environment).await?;
        check_held(&held.declarations)?;

        if at != held.revision.as_ref() {
            return Err(DesiredStateError::Conflict.into());
        }

        let placed = repository.read_placements(environment).await?;
        check_held_placements(&placed.placements, &held.declarations)?;

        // A tenant with two logical data sources both shared on the source
        // being removed (B2: legitimate, not a collision) must still be
        // named once, not once per placement.
        let tenants: Vec<TenantId> = placed
            .placements
            .iter()
            .filter(|placement| &placement.data_source == id)
            .map(|placement| placement.tenant.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        if !tenants.is_empty() {
            return Err(PlatformError::DataSourceInUse {
                id: id.clone(),
                tenants,
            });
        }

        if !held.declarations.iter().any(|declared| &declared.id == id) {
            return Ok(Declared::Unchanged(held));
        }

        let declarations: Vec<DataSourceDeclaration> = held
            .declarations
            .iter()
            .filter(|declared| &declared.id != id)
            .cloned()
            .collect();

        let message = format!("Remove {id} from {environment}");

        repository
            .write_environment(
                environment,
                EnvironmentWrite {
                    data_sources: (&declarations, at),
                    placements: (&placed.placements, placed.revision.as_ref()),
                },
                &message,
            )
            .await?;

        Ok(Declared::Written(
            repository.read_data_sources(environment).await?,
        ))
    }
}
