//! One desired-state change, as one commit.

use crate::host::{PlatformGitRepository, RefUpdate};
use crate::{CommitRevision, FileChange, PlatformGitError};

/// How many times a write is rebuilt on a moved head before giving up.
///
/// Bounded, because the platform repository is a shared branch and an
/// unbounded retry is a reconciliation loop that never converges and never
/// says so. Four is generous against a repository a handful of people and one
/// automatic policy write to; exhausting it is a real signal that something is
/// committing continuously, and [`Contended`](PlatformGitError::Contended)
/// says so rather than hiding it.
const ATTEMPTS: usize = 4;

impl PlatformGitRepository {
    /// Applies every change in one commit, or applies none of them.
    ///
    /// `base` is the commit the caller read the current content at, and each
    /// change carries the revision the caller believed it was editing.
    ///
    /// # What happens when somebody else commits
    ///
    /// ```text
    /// 409 from the ref update
    ///  ↓
    /// re-read the head, and the revisions of the paths being written
    ///  ├─ unchanged → rebuild the tree and commit on the new head, retry
    ///  └─ changed   → Conflict, and nothing is written
    /// ```
    ///
    /// So an unrelated commit costs a retry and never reaches an operator,
    /// and a change to a file this write is editing is refused rather than
    /// overwritten.
    ///
    /// # Errors
    ///
    /// [`Conflict`](PlatformGitError::Conflict) if a path being written moved,
    /// [`Contended`](PlatformGitError::Contended) if the branch kept moving
    /// for unrelated reasons, and the transport variants otherwise.
    ///
    /// A transport failure is **not** retried here: if the ref update timed
    /// out after the host had applied it, retrying inside this call would find
    /// the paths carrying their new revisions and report a conflict against
    /// the caller's own change. Reporting `Unavailable` instead leaves the
    /// caller to re-read, at which point it finds its change already applied
    /// and has nothing to do — idempotent one level up, where the information
    /// to be idempotent actually exists.
    pub async fn update_files_atomically(
        &self,
        base: &CommitRevision,
        changes: &[FileChange],
        message: &str,
    ) -> Result<CommitRevision, PlatformGitError> {
        if changes.is_empty() {
            // Refused rather than quietly succeeding. A caller that meant to
            // change nothing should not be minting empty commits on the branch
            // an environment follows.
            return Err(PlatformGitError::Rejected {
                detail: "a desired-state change with no files".to_owned(),
            });
        }

        // Once, outside the loop. Blobs are content-addressed, so they do not
        // depend on which head the change lands on, and rebuilding them per
        // attempt would be a round trip per file per retry for identical
        // hashes.
        let mut entries = Vec::with_capacity(changes.len());
        for change in changes {
            entries.push((change.path.clone(), self.create_blob(&change.text).await?));
        }

        let mut base = base.clone();

        for attempt in 1..=ATTEMPTS {
            let base_tree = self.tree_of(&base).await?;
            let tree = self.create_tree(&base_tree, &entries).await?;
            let commit = self.create_commit(message, &tree, &base).await?;

            if self.update_ref(&commit).await? == RefUpdate::Applied {
                return Ok(commit);
            }

            // The head moved. Whether that matters is a question about the
            // paths being written, not about the branch.
            base = self.head().await?;
            self.refuse_if_a_written_path_moved(changes, &base).await?;

            tracing::debug!(
                attempt,
                head = base.as_str(),
                "the branch moved under a desired-state write; rebuilding"
            );
        }

        Err(PlatformGitError::Contended)
    }

    /// Refuses if any path being written is no longer what the caller read.
    ///
    /// Absence counts as a change in both directions: a file that has been
    /// deleted has moved, and so has one that has appeared where the caller
    /// expected none.
    async fn refuse_if_a_written_path_moved(
        &self,
        changes: &[FileChange],
        at: &CommitRevision,
    ) -> Result<(), PlatformGitError> {
        for change in changes {
            if self.revision_at(&change.path, at).await? != change.expected {
                return Err(PlatformGitError::Conflict {
                    path: change.path.clone(),
                });
            }
        }

        Ok(())
    }
}
