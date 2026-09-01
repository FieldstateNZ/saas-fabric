//! Moving one component's desired state, as one commit.

use std::collections::BTreeMap;

mod plan;

use crate::host::PlatformGitRepository;
use crate::{CommitRevision, PlatformGitError};

/// One image's new identity, as the caller resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDigest {
    /// Where the image was published. Checked against what the manifest
    /// already declares for this role, and refused if it disagrees — a caller
    /// may move a component to a new *version*, never to a new registry.
    pub repository: String,

    /// The immutable digest to deploy.
    pub digest: String,
}

/// What a caller asks a component to move to.
///
/// A release unit: one version, one source commit, and every image the
/// component publishes. Not a per-image update — promoting the console without
/// the control plane would put two thirds of a release on an environment and
/// call it integrated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentVersion {
    /// The version, once.
    pub version: String,

    /// The commit every image was built from.
    pub source_revision: String,

    /// Images by role. Must be exactly the roles the manifest declares.
    pub images: BTreeMap<String, ImageDigest>,
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

        changes.push(crate::FileChange {
            path,
            text: read.document.render()?,
            expected: Some(read.stored.revision),
        });

        self.update_files_atomically(&read.head, &changes, message).await
    }
}
