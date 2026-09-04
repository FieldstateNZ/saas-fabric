//! The write both desired-state operations are.

use fabric_platform_management::DesiredRevision;

use crate::desired::{identity, plan, HoldChange, WantedVersion};
use crate::host::PlatformGitRepository;
use crate::{CommitRevision, PlatformGitError};

impl PlatformGitRepository {
    /// The write both of those are.
    pub(super) async fn write_desired(
        &self,
        environment: &str,
        component: &str,
        wanted: &WantedVersion,
        hold: &HoldChange,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        let mut read = self.read_manifest(environment).await?;
        let path = read.stored.path.clone();

        // The decision was taken against `at`. If desired state has moved
        // since — a hold added, a policy changed, a version somebody else
        // wrote — then what is about to be applied was decided about something
        // else, and the `expected` revision below would happily overwrite it.
        //
        // This is where the selector's claim that "concurrent changes are
        // enforced by the write" is actually made true. Comparing the
        // adapter's own read against itself, which is what this used to do,
        // proves only that nothing changed during the write.
        if read.stored.revision.as_str() != at.as_str() {
            return Err(PlatformGitError::Conflict { path: path.clone() });
        }

        let roots = read.document.manifest.managed_roots.clone();
        let entry = read
            .document
            .manifest
            .components
            .get_mut(component)
            .ok_or_else(|| PlatformGitError::Rejected {
                detail: format!("{path} does not know the component '{component}'"),
            })?;

        identity::check_release(component, entry, wanted)?;

        let mut changes = plan::rewrite_pins(self, &read.head, component, entry, wanted, &roots).await?;
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
