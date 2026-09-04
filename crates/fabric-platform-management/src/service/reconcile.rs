//! Acting on what a component's situation turns out to be.

use crate::Release;
use crate::{
    decide, ComponentStatus, Decision, DesiredStateStatus, PlatformError, PlatformManagement, Reconciliation,
};

impl PlatformManagement {
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
    /// [`Conflict`](crate::DesiredStateError::Conflict) when the decision was taken
    /// against desired state that has since moved — which is an instruction to
    /// decide again, not a failure of the component.
    pub async fn reconcile(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<Reconciliation, PlatformError> {
        let (desired, discovery) = self.look(environment, component).await?;
        let was = desired.version.clone();

        let Decision::Advance(release) = decide(
            desired.policy,
            desired.channel,
            desired.hold.is_some(),
            &discovery,
        ) else {
            return Ok(Reconciliation {
                was,
                status: ComponentStatus::assemble(component, &desired, &discovery),
            });
        };

        // A chart has no commit to name, so the message does not pretend to
        // one. What a release says about itself is the release's to say.
        let message = match &release {
            Release::Unit(unit) => format!(
                "Advance {environment} to {component} {}\n\nBuilt from {}.",
                unit.version, unit.source_revision
            ),
            Release::Chart { version, .. } => {
                format!("Advance {environment} to {component} chart {version}")
            }
        };

        self.desired_state
            .advance(environment, component, &release, &desired.revision, &message)
            .await?;

        // Reported from what was written rather than by reading again: a
        // second read would race the very reconciliation this just performed,
        // and answering "what did I just do" from a fresh query is how a
        // console shows somebody else's change as though it were yours.
        let mut moved = desired.clone();
        moved.version = release.version().clone();

        Ok(Reconciliation {
            was,
            status: ComponentStatus {
                desired_state: DesiredStateStatus::Current,
                // Nothing, having just taken it. `assemble` would report the
                // version this pass advanced *to*, which under this field's
                // meaning would claim there is something newer than the
                // version it also reports as desired.
                newer: None,
                ..ComponentStatus::assemble(component, &moved, &discovery)
            },
        })
    }
}
