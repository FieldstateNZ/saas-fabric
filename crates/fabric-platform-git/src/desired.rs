//! Moving one component's desired state, as one commit.

mod identity;
mod inputs;
mod plan;
mod render;
mod write;

pub use inputs::{ComponentVersion, ImageDigest, WantedVersion};

use fabric_platform_management::{DesiredRevision, Hold};

use crate::host::PlatformGitRepository;
use crate::{CommitRevision, PlatformGitError};

/// What a desired-state write does to the component's hold.
///
/// Advancing keeps whatever is there — clearing a hold to succeed is exactly
/// what the selector must not be able to do. Rolling back sets one, in the
/// same commit that moves the version, because an environment moved backwards
/// with advancement still live would be advanced forward again on the next
/// sweep.
///
/// This is the adapter's private vocabulary. The *port* states the guarantee
/// where every caller meets it: `advance` has no hold parameter at all.
enum HoldChange {
    /// Leave it exactly as the manifest has it.
    Keep,

    /// Replace it.
    Set(Hold),
}

impl PlatformGitRepository {
    /// Reads an environment's manifest as it stands on the branch.
    ///
    /// Through the same reader the writes use, so a manifest that describes
    /// some *other* environment is refused here too. It used to be accepted on
    /// this path, which meant a console could report a different environment's
    /// components under this one's name — a read, and still a lie.
    ///
    /// # Errors
    ///
    /// [`PlatformGitError`] if the branch or the manifest cannot be read, the
    /// manifest is not a shape this understands, or it describes somewhere
    /// else.
    pub async fn components_manifest(&self, environment: &str) -> Result<crate::Manifest, PlatformGitError> {
        Ok(self.read_manifest(environment).await?.document.manifest)
    }

    /// Points a component at a version, in one commit.
    ///
    /// Reads the environment's manifest, works out which files carry the pins
    /// from what that manifest declares, and writes the manifest and those
    /// files together. Nothing here knows the platform repository's layout;
    /// `pinnedIn` is where the layout is stated, and this refuses to write
    /// anything it does not name.
    ///
    /// The component's **policy and any hold are carried through untouched.**
    /// Advancing a held component is a decision this does not make: it writes
    /// what it is told, and whether it should have been told is the selector's
    /// question.
    ///
    /// # Errors
    ///
    /// [`Rejected`](PlatformGitError::Rejected) if the manifest does not
    /// describe this environment, does not know the component, disagrees about
    /// which images it publishes or where they come from, or declares a path
    /// this will not write.
    /// [`Conflict`](PlatformGitError::Conflict) if any of those files changed
    /// while this was preparing the commit, and the transport variants
    /// otherwise.
    pub async fn set_component_desired_state(
        &self,
        environment: &str,
        component: &str,
        wanted: &WantedVersion,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        self.write_desired(environment, component, wanted, &HoldChange::Keep, at, message)
            .await
    }

    /// Points a component at an older version *and* holds it there, in one
    /// commit.
    ///
    /// # It takes either shape, and checks which
    ///
    /// It used to take a `ComponentVersion` and wrap it in
    /// [`WantedVersion::Images`], which made "images only" a property of this
    /// signature. Rolling back is now offered for a chart too, so the shape a
    /// caller brings is theirs to bring — and the same identity check every
    /// other write runs refuses one that does not match what the component
    /// publishes, before any file is read.
    ///
    /// # Errors
    ///
    /// The same as [`set_component_desired_state`](Self::set_component_desired_state).
    pub async fn roll_back_component(
        &self,
        environment: &str,
        component: &str,
        wanted: &WantedVersion,
        hold: &Hold,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        self.write_desired(
            environment,
            component,
            wanted,
            &HoldChange::Set(hold.clone()),
            at,
            message,
        )
        .await
    }
}
