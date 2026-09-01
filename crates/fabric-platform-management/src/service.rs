//! Reading a component's situation, and acting on it.

use std::sync::Arc;

#[cfg(test)]
mod service_tests;

use crate::{
    decide, discover, ComponentDesired, ComponentStatus, Decision, DesiredState, DesiredStateError,
    DesiredStateStatus, Discovery, Reconciliation, Registry, RegistryError,
};

/// What can go wrong looking at, or moving, a component.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlatformError {
    /// Desired state could not be read or written.
    #[error(transparent)]
    DesiredState(#[from] DesiredStateError),

    /// A registry could not be asked.
    ///
    /// Kept distinct because it is the failure that changes nothing: desired
    /// state is untouched and availability is merely stale.
    #[error(transparent)]
    Registry(#[from] RegistryError),
}

/// Platform Management, over whatever implements its two ports.
pub struct PlatformManagement {
    /// Where published artifacts are looked up.
    registry: Arc<dyn Registry>,

    /// Where an environment's desired state is kept.
    desired_state: Arc<dyn DesiredState>,
}

impl PlatformManagement {
    /// Builds the service over its two ports.
    #[must_use]
    pub fn new(registry: Arc<dyn Registry>, desired_state: Arc<dyn DesiredState>) -> Self {
        Self {
            registry,
            desired_state,
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

    /// Acts on a component's situation, if its policy says to.
    ///
    /// Reads, discovers, decides, and writes only on
    /// [`Decision::Advance`](crate::Decision::Advance). The result describes
    /// the state after any write *and* where it started, so a caller can tell
    /// "advanced" from "was already there" — which produce the same status and
    /// are not the same event.
    ///
    /// # Errors
    ///
    /// [`PlatformError`], including
    /// [`Conflict`](DesiredStateError::Conflict) when the decision was taken
    /// against desired state that has since moved — which is an instruction to
    /// decide again, not a failure of the component.
    pub async fn reconcile(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<Reconciliation, PlatformError> {
        let (desired, discovery) = self.look(environment, component).await?;
        let was = desired.version.clone();

        let Decision::Advance(unit) = decide(desired.policy, desired.hold.is_some(), &discovery) else {
            return Ok(Reconciliation {
                was,
                status: ComponentStatus::assemble(component, &desired, &discovery),
            });
        };

        let message = format!(
            "Advance {environment} to {component} {}\n\nBuilt from {}.",
            unit.version, unit.source_revision
        );
        self.desired_state
            .advance(environment, component, &unit, &message)
            .await?;

        // Reported from what was written rather than by reading again: a
        // second read would race the very reconciliation this just performed,
        // and answering "what did I just do" from a fresh query is how a
        // console shows somebody else's change as though it were yours.
        let mut moved = desired.clone();
        moved.version = unit.version.clone();

        Ok(Reconciliation {
            was,
            status: ComponentStatus {
                desired_state: DesiredStateStatus::Current,
                available: Some(unit.version),
                ..ComponentStatus::assemble(component, &moved, &discovery)
            },
        })
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
