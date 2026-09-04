//! Putting an environment back on something it ran before.

use crate::service::brake::ROLLBACK;
use crate::service::{backwards, PlatformError, PlatformManagement};
use crate::{ComponentStatus, Discovery, History, Hold};

impl PlatformManagement {
    /// What this component could be rolled back to.
    ///
    /// # What rollback means, and why it is offered for both kinds
    ///
    /// Rolling back restores a **previously selected desired version**. For
    /// images it also restores the exact bytes, because a release unit carries
    /// every digest. For a chart it restores the version, and a chart
    /// repository can republish the bytes behind a version — so the environment
    /// comes back to what it was *asked* to run rather than provably to what it
    /// ran. That difference is stated to the operator, in the console and in
    /// the architecture doc, rather than being a reason to offer nothing.
    ///
    /// The alternative definition — rollback requires immutable artifact
    /// identity, so a chart cannot have one — was considered and not taken. It
    /// is coherent, and it leaves an operator whose chart upgrade went wrong
    /// with no route back but a hand edit of the platform repository, which is
    /// the break-glass path and not an operator experience.
    ///
    /// Observed, not remembered. The list is what the registry or the chart
    /// repository holds *now* — so a version withdrawn since it ran is not
    /// offered, and for images neither is one whose images never agreed.
    /// Reading it changes nothing.
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

        Ok(backwards::candidates(self.registry.as_ref(), self.charts.as_ref(), &desired).await?)
    }

    /// Puts a component back on an older version, and holds it there.
    ///
    /// # The caller names a version and nothing else
    ///
    /// What gets written is resolved here, from the registry or the chart
    /// index, **on this request** — not carried over from whatever the
    /// candidates listing returned moments ago. For images the version, the
    /// source commit and every digest are assembled together, so a caller
    /// cannot supply a digest, cannot move one image, and cannot name a
    /// version that is not a complete coherent release. For a chart the
    /// version travels with the repository and chart name it was discovered
    /// under, so a number that is plausible against the wrong chart is still
    /// refused by the write.
    ///
    /// Re-resolving is not redundant with the listing. It is what makes a
    /// version withdrawn between the two requests a refusal rather than a
    /// deployment from a stale candidate object, and it is why the request
    /// body carries a name instead of a release.
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

        let release = backwards::one(self.registry.as_ref(), self.charts.as_ref(), &desired, version)
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
                &release,
                &hold,
                &desired.revision,
                &format!(
                    "Roll {component} in {environment} back to {}",
                    release.version().as_str()
                ),
            )
            .await?;

        let mut settled = desired.clone();
        settled.version = release.version().clone();
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
