//! Stopping an environment advancing, and letting it go again.

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::service::{PlatformError, PlatformManagement};
use crate::{ComponentStatus, Discovery, Hold, UpdatePolicy};

/// Why Fabric wrote a hold, as it records it in the manifest.
///
/// A closed vocabulary rather than operator text. The reason is what a later
/// reader — a person, or a future version of this — decides from, and a free
/// field would mean deciding on prose. What an operator wants to say goes in
/// the note, which nothing branches on.
const PAUSED: &str = "paused";

impl PlatformManagement {
    /// Stops a component advancing, leaving the version it runs alone.
    ///
    /// # Pause is not rollback, and not a policy change
    ///
    /// The desired version does not move, so nothing redeploys: an operator
    /// who wants to stop advancement before testing something has said only
    /// that. And the policy stays `Automatic` — they did not decide the
    /// component should stop advancing forever, and the manifest must not
    /// record that they did. Effective state reads `Automatic — Paused`.
    ///
    /// # Errors
    ///
    /// [`PlatformError::NotAdvancing`] if the component does not advance on
    /// its own, and the desired-state variants otherwise — including
    /// [`NotFound`](crate::DesiredStateError::NotFound) when the environment's
    /// manifest does not name this component.
    pub async fn pause(
        &self,
        environment: &str,
        component: &str,
        note: Option<&str>,
    ) -> Result<ComponentStatus, PlatformError> {
        // The identifier rule: a caller may select something the manifest
        // already names, and this read is what enforces it. What comes back is
        // trusted desired state; the name itself reaches no path, no registry
        // and no other locator.
        let desired = self.desired_state.component(environment, component).await?;

        if desired.policy != UpdatePolicy::Automatic {
            return Err(PlatformError::NotAdvancing {
                component: component.to_owned(),
            });
        }

        let hold = Hold {
            reason: PAUSED.to_owned(),
            since: self.stamp(),
            note: note.map(ToOwned::to_owned),
        };

        self.desired_state
            .pause(
                environment,
                component,
                &hold,
                &format!("Pause {component} in {environment}"),
            )
            .await?;

        Ok(Self::after(component, &desired, Some(hold)))
    }

    /// Lets a component advance again.
    ///
    /// # It permits, it does not advance
    ///
    /// Resuming writes one thing: the hold is gone. What happens next is the
    /// next sweep's to decide, from what it observes then — so an operator who
    /// resumes and immediately reloads sees the environment as it is, not a
    /// version this pretended to move it to.
    ///
    /// # Errors
    ///
    /// The desired-state variants, including
    /// [`NotFound`](crate::DesiredStateError::NotFound) when the environment's
    /// manifest does not name this component.
    pub async fn resume(&self, environment: &str, component: &str) -> Result<ComponentStatus, PlatformError> {
        let desired = self.desired_state.component(environment, component).await?;

        // Nothing to lift, so nothing to write. An empty commit would be a
        // change in the history of a repository whose history is the audit
        // trail, recording that somebody clicked rather than that anything
        // happened.
        if desired.hold.is_none() {
            return Ok(Self::after(component, &desired, None));
        }

        self.desired_state
            .resume(
                environment,
                component,
                &format!("Resume {component} in {environment}"),
            )
            .await?;

        Ok(Self::after(component, &desired, None))
    }

    /// Now, as the manifest records it.
    ///
    /// RFC 3339 in UTC, because this lands in a file people read and hand-edit
    /// under break-glass. A clock that cannot be represented — which needs a
    /// timestamp outside the range of a calendar date — records nothing rather
    /// than a wrong instant: a hold with an unreadable `since` is still a hold,
    /// and the field is for a human, not for a decision.
    fn stamp(&self) -> String {
        OffsetDateTime::from_unix_timestamp(i64::try_from(self.clock.now_unix_seconds()).unwrap_or(i64::MAX))
            .ok()
            .and_then(|at| at.format(&Rfc3339).ok())
            .unwrap_or_default()
    }

    /// What the component is, once the hold has been written.
    ///
    /// Assembled from what this just did rather than by reading again. A
    /// second read would race the write, and answering "what did I just do"
    /// from a fresh query is how a console shows somebody else's change as
    /// though it were yours.
    ///
    /// Discovery is deliberately empty: pausing asked no registry anything, so
    /// claiming to know what is newer would be reporting a fact this did not
    /// observe. The next sweep fills it in.
    fn after(component: &str, desired: &crate::ComponentDesired, hold: Option<Hold>) -> ComponentStatus {
        let mut settled = desired.clone();
        settled.hold = hold;

        ComponentStatus::assemble(component, &settled, &Discovery::default())
    }
}
