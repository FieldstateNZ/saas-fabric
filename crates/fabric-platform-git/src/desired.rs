//! Moving one component's desired state, as one commit.

mod inputs;
mod plan;

pub use inputs::{ComponentVersion, ImageDigest};

use fabric_platform_management::Hold;

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
        wanted: &ComponentVersion,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        self.write_desired(environment, component, wanted, &HoldChange::Keep, message)
            .await
    }

    /// Points a component at an older version *and* holds it there, in one
    /// commit.
    ///
    /// # Errors
    ///
    /// The same as [`set_component_desired_state`](Self::set_component_desired_state).
    pub async fn roll_back_component(
        &self,
        environment: &str,
        component: &str,
        wanted: &ComponentVersion,
        hold: &Hold,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        self.write_desired(
            environment,
            component,
            wanted,
            &HoldChange::Set(hold.clone()),
            message,
        )
        .await
    }

    /// The write both of those are.
    async fn write_desired(
        &self,
        environment: &str,
        component: &str,
        wanted: &ComponentVersion,
        hold: &HoldChange,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        let mut read = self.read_manifest(environment).await?;
        let path = read.stored.path.clone();

        let roots = read.document.manifest.managed_roots.clone();
        let entry = read
            .document
            .manifest
            .components
            .get_mut(component)
            .ok_or_else(|| PlatformGitError::Rejected {
                detail: format!("{path} does not know the component '{component}'"),
            })?;

        plan::check_release_unit(component, entry, wanted)?;

        let mut changes = plan::rewrite_pins(self, &read.head, entry, wanted, &roots).await?;
        plan::apply(entry, wanted);

        if let HoldChange::Set(hold) = hold {
            entry.hold = Some(hold.clone());
        }

        changes.push(crate::FileChange {
            path,
            text: read.document.render()?,
            expected: Some(read.stored.revision),
        });

        self.update_files_atomically(&read.head, &changes, message).await
    }
}
