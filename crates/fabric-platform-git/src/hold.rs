//! Pausing and resuming a component, as one commit each.

use fabric_platform_management::Hold;

use crate::host::PlatformGitRepository;
use crate::{CommitRevision, PlatformGitError};

impl PlatformGitRepository {
    /// Sets or clears a component's hold, and changes nothing else.
    ///
    /// # One file, structurally
    ///
    /// A hold does not move a version, so no deployment overlay is touched —
    /// and this cannot touch one, because it never asks what `pinnedIn`
    /// declares. Pausing an environment stops it moving; it does not move it,
    /// and there is no path here by which it could.
    ///
    /// # It is the only thing that changes
    ///
    /// The version, the source revision, every image digest, the channel and
    /// the policy are all carried through by the document round-trip. A hold
    /// is a pause, not a policy change: an operator who paused did not say
    /// "stop advancing forever", and this must not record that they did.
    ///
    /// # Errors
    ///
    /// [`Rejected`](PlatformGitError::Rejected) if the manifest does not
    /// describe this environment or does not know the component.
    /// [`Conflict`](PlatformGitError::Conflict) if the manifest changed while
    /// this was preparing the commit, and the transport variants otherwise.
    pub async fn set_component_hold(
        &self,
        environment: &str,
        component: &str,
        hold: Option<&Hold>,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        let mut read = self.read_manifest(environment).await?;
        let path = read.stored.path.clone();

        // A lookup, and only a lookup. The name an operator chose selects an
        // entry in a document this platform already trusts; it reaches no
        // path, no registry and no other locator.
        let entry = read
            .document
            .manifest
            .components
            .get_mut(component)
            .ok_or_else(|| PlatformGitError::Rejected {
                detail: format!("{path} does not know the component '{component}'"),
            })?;

        entry.hold = hold.cloned();

        let changes = vec![crate::FileChange {
            path,
            text: read.document.render()?,
            expected: Some(read.stored.revision),
        }];

        self.update_files_atomically(&read.head, &changes, message).await
    }
}
