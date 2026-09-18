//! Declaring an environment's data sources, and reading what is declared.

#[cfg(test)]
#[path = "service_tests.rs"]
mod service_tests;

use std::sync::Arc;

use crate::data_sources::declaration::DataSourceDeclaration;
use crate::data_sources::held::check_held;
use crate::data_sources::plan::{plan, Plan};
use crate::data_sources::port::DataSourceState;
use crate::data_sources::read::DataSourcesRead;
use crate::{DesiredRevision, DesiredStateError, PlatformError};

/// What `DataSources::declare` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Declared {
    /// The declaration changed what was held. The read that followed the
    /// write.
    Written(DataSourcesRead),

    /// Nothing about the declaration differed from what was already held,
    /// so nothing was written. The read that found that out.
    Unchanged(DataSourcesRead),
}

/// Declaring an environment's data sources, over whatever holds them.
pub struct DataSources {
    /// Where declared data sources are read and written.
    state: Arc<dyn DataSourceState>,
}

impl DataSources {
    /// Builds the service over its port.
    #[must_use]
    pub fn new(state: Arc<dyn DataSourceState>) -> Self {
        Self { state }
    }

    /// Every data source an environment declares, changing nothing.
    ///
    /// # Errors
    ///
    /// `PlatformError` if the environment cannot be read, or
    /// [`PlatformError::InvalidHeldDataSources`] if a held entry is no
    /// longer valid -- a break-glass edit made it invalid, or gave two
    /// entries one id.
    pub async fn list(&self, environment: &str) -> Result<DataSourcesRead, PlatformError> {
        let read = self.state.read_data_sources(environment).await?;
        check_held(&read.declarations)?;

        Ok(read)
    }

    /// Declares or corrects one data source and returns the new read.
    ///
    /// The incoming declaration's revision is ignored and computed: 1 for
    /// a new id, held + 1 when any field differs, unchanged -- and no
    /// write at all -- when nothing differs. at is the revision the
    /// caller read; the write refuses if it does not match what is held.
    ///
    /// # Why the precondition is checked before planning, not only on write
    ///
    /// A declaration identical to what is already held plans as
    /// `Plan::Unchanged`, which never reaches `write_data_sources` -- so a
    /// precondition check living only inside the write would never run for
    /// it, and a caller whose `at` names state that has since moved would
    /// be told `200 Unchanged` instead of the conflict they were owed. The
    /// content happening to match what is now held does not mean the
    /// caller read that state; checking `at` against `held.revision` here,
    /// before `plan` ever runs, is what makes a stale precondition refused
    /// whether or not it would have changed anything.
    ///
    /// # Why this reads again after a successful write
    ///
    /// `write_data_sources` answers only whether the write succeeded, not
    /// the revision the platform repository assigned it -- the same
    /// opaque token `DesiredState` never invents either. Reading again is
    /// the only honest way to hand the caller something to send back as
    /// its next If-Match.
    ///
    /// # Errors
    ///
    /// `PlatformError::InvalidDataSource` if the declaration breaks one of
    /// ADR 0023 part 1's rules, [`DesiredStateError::Conflict`] if `at`
    /// does not match what is held, and
    /// [`PlatformError::InvalidHeldDataSources`] if what is held is no
    /// longer valid -- see [`Self::list`].
    pub async fn declare(
        &self,
        environment: &str,
        declaration: DataSourceDeclaration,
        at: Option<&DesiredRevision>,
    ) -> Result<Declared, PlatformError> {
        declaration.validate()?;

        let held = self.state.read_data_sources(environment).await?;
        check_held(&held.declarations)?;

        if at != held.revision.as_ref() {
            return Err(DesiredStateError::Conflict.into());
        }

        let Plan::Write(declarations) = plan(&held.declarations, &declaration) else {
            return Ok(Declared::Unchanged(held));
        };

        let already_declared = held.declarations.iter().any(|item| item.id == declaration.id);
        let verb = if already_declared { "Correct" } else { "Declare" };
        let message = format!("{verb} {} in {environment}", declaration.id);

        self.state
            .write_data_sources(environment, &declarations, at, &message)
            .await?;

        Ok(Declared::Written(
            self.state.read_data_sources(environment).await?,
        ))
    }
}
