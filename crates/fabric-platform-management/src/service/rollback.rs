//! Putting an environment back on something it ran before.

use crate::service::brake::ROLLBACK;
use crate::service::rollable::rollable;
use crate::service::{PlatformError, PlatformManagement};
use crate::{history, resolve, ComponentStatus, Discovery, History, Hold};

impl PlatformManagement {
    /// What this component could be rolled back to.
    ///
    /// Observed, not remembered. The list is what the registry holds *now*,
    /// resolved to whole release units — so a version withdrawn since it ran
    /// is not offered, and neither is one whose images never agreed. Reading
    /// it changes nothing.
    ///
    /// # Errors
    ///
    /// The desired-state variants if the component cannot be read, and
    /// [`PlatformError::Registry`] if a registry could not be asked. Nothing
    /// is offered from a partial answer.
    pub async fn rollback_candidates(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<History, PlatformError> {
        let desired = self.desired_state.component(environment, component).await?;
        let repositories = rollable(component, &desired)?;

        Ok(history(
            self.registry.as_ref(),
            repositories,
            desired.channel,
            Some(&desired.version),
            &desired.version,
        )
        .await?)
    }

    /// Puts a component back on an older version, and holds it there.
    ///
    /// # The caller names a version and nothing else
    ///
    /// What gets written is resolved here, from the registry, **on this
    /// request** — not carried over from whatever the candidates listing
    /// returned moments ago. The version, the source commit and three image
    /// digests are assembled together, so a caller cannot supply a digest,
    /// cannot move one image, and cannot name a version that is not a
    /// complete coherent release.
    ///
    /// Re-resolving is not redundant with the listing. It is what makes a
    /// version withdrawn between the two requests a refusal rather than a
    /// deployment from a stale candidate object, and it is why the request
    /// body carries a name instead of a unit.
    ///
    /// # The hold is not optional
    ///
    /// Moving an environment backwards under a live automatic policy would be
    /// undone by the next sweep, and the operator would watch their rollback
    /// disappear. So the hold travels in the same commit, and advancement
    /// stays paused until somebody resumes. It is a `rollback` hold rather
    /// than a `paused` one, because a later reader should be able to tell
    /// which act stopped the environment.
    ///
    /// The policy is untouched. Rolling back is "put me here and stay until I
    /// say otherwise", which is not a decision to stop advancing forever.
    ///
    /// # Errors
    ///
    /// [`PlatformError::NotRollable`] if the version is not one this component
    /// can be rolled back to, and the desired-state and registry variants
    /// otherwise.
    pub async fn roll_back(
        &self,
        environment: &str,
        component: &str,
        version: &str,
        note: Option<&str>,
    ) -> Result<ComponentStatus, PlatformError> {
        let desired = self.desired_state.component(environment, component).await?;
        let repositories = rollable(component, &desired)?;

        // Resolved, not looked up in a list. One version costs a fifth of
        // what re-deriving the whole listing does, and this request also has a
        // Git write to pay for — doing both is what returned `504` against the
        // real registry.
        let unit = resolve(
            self.registry.as_ref(),
            repositories,
            desired.channel,
            Some(&desired.version),
            &desired.version,
            version,
        )
        .await?
        .ok_or_else(|| PlatformError::NotRollable {
            component: component.to_owned(),
            version: version.to_owned(),
        })?;

        let hold = Hold {
            reason: ROLLBACK.to_owned(),
            since: self.stamp(),
            note: note.map(ToOwned::to_owned),
        };

        self.desired_state
            .roll_back(
                environment,
                component,
                &unit,
                &hold,
                &format!(
                    "Roll {component} in {environment} back to {}",
                    unit.version.as_str()
                ),
            )
            .await?;

        let mut settled = desired.clone();
        settled.version = unit.version.clone();
        settled.hold = Some(hold);

        // Discovery is deliberately empty. This pass looked *backwards*; what
        // is newer is a different question, and answering it from a search
        // that never asked would be reporting something unobserved.
        Ok(ComponentStatus::assemble(
            component,
            &settled,
            &Discovery::default(),
        ))
    }
}
