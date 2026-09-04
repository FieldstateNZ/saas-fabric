//! Reading a component's situation, and acting on it.

use std::sync::Arc;

use fabric_core::Clock;

mod backwards;
mod brake;
mod errors;
mod look;
mod reconcile;
mod rollback;

#[cfg(test)]
mod service_tests;

pub use errors::PlatformError;

use crate::{ChartIndex, ComponentStatus, DesiredState, Registry};

/// Platform Management, over whatever implements its two ports.
pub struct PlatformManagement {
    /// Where published artifacts are looked up.
    registry: Arc<dyn Registry>,

    /// Where chart versions are looked up.
    ///
    /// A second port rather than a second implementation of the first: a chart
    /// repository lists versions and nothing else, and the registry port
    /// promises a digest and a provenance it cannot supply.
    charts: Arc<dyn ChartIndex>,

    /// Where an environment's desired state is kept.
    desired_state: Arc<dyn DesiredState>,

    /// Stamps the record a human reads.
    clock: Arc<dyn Clock>,
}

impl PlatformManagement {
    /// Builds the service over its two ports.
    #[must_use]
    pub fn new(
        registry: Arc<dyn Registry>,
        charts: Arc<dyn ChartIndex>,
        desired_state: Arc<dyn DesiredState>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            registry,
            charts,
            desired_state,
            clock,
        }
    }

    /// What a component's situation is, changing nothing.
    ///
    /// # This never writes, and that is a contract
    ///
    /// It is what the console reads. Opening a page should not be able to move
    /// an environment: a read with mutation semantics is a read nobody can
    /// perform safely, including a refresh, a health check, or a second
    /// operator looking at the same screen.
    ///
    /// Advancement happens in [`reconcile`](Self::reconcile), and only there.
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if desired state cannot be read or a registry cannot
    /// be asked.
    pub async fn status(&self, environment: &str, component: &str) -> Result<ComponentStatus, PlatformError> {
        let (desired, discovery) = self.look(environment, component).await?;

        Ok(ComponentStatus::assemble(component, &desired, &discovery))
    }

    /// Every component of an environment, changing nothing.
    ///
    /// The console's read. Like [`status`](Self::status) it cannot write, and
    /// for the same reason: a page that moved an environment when it loaded
    /// would be a page nobody could open twice.
    ///
    /// # Errors
    ///
    /// [`PlatformError`] if the environment cannot be read. A component that
    /// cannot be read fails the whole call — unlike a sweep, which is looking
    /// after components and must not abandon the rest, this is answering a
    /// question and a partial answer would be read as a complete one.
    pub async fn statuses(&self, environment: &str) -> Result<Vec<ComponentStatus>, PlatformError> {
        let components = self.desired_state.components(environment).await?;
        let mut statuses = Vec::with_capacity(components.len());

        for component in components {
            statuses.push(self.status(environment, &component).await?);
        }

        Ok(statuses)
    }

    /// The clock records are stamped with.
    pub(crate) fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    /// The port desired state is read and written through.
    pub(crate) fn desired_state(&self) -> &dyn DesiredState {
        self.desired_state.as_ref()
    }
}
