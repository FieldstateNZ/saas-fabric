//! Reading a component's situation, and acting on it.

use std::sync::Arc;

use fabric_core::Clock;

mod errors;
mod reconcile;

#[cfg(test)]
mod service_tests;

pub use errors::PlatformError;

use crate::{discover, ComponentDesired, ComponentStatus, DesiredState, Discovery, Registry};

/// Platform Management, over whatever implements its two ports.
pub struct PlatformManagement {
    /// Where published artifacts are looked up.
    registry: Arc<dyn Registry>,

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
        desired_state: Arc<dyn DesiredState>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            registry,
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

    /// The clock records are stamped with.
    pub(crate) fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    /// The port desired state is read and written through.
    pub(crate) fn desired_state(&self) -> &dyn DesiredState {
        self.desired_state.as_ref()
    }

    /// Reads desired state and asks the registries what exists.
    async fn look(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<(ComponentDesired, Discovery), PlatformError> {
        let desired = self.desired_state.component(environment, component).await?;

        // The series is the desired version's own line. An automatic policy
        // walks forward within it; moving to a new line is a deliberate act,
        // not something discovery does on a Tuesday.
        let discovery = discover(
            self.registry.as_ref(),
            &desired.repositories,
            desired.channel,
            Some(&desired.version),
            &desired.version,
        )
        .await?;

        Ok((desired, discovery))
    }
}
