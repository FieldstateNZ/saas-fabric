//! Working out what a version change means for the files that carry it.

use std::collections::BTreeMap;

use crate::components::{check_writable, repin, Component};
use crate::desired::ComponentVersion;
use crate::host::PlatformGitRepository;
use crate::{CommitRevision, FileChange, PlatformGitError};

/// Refuses a request that is not this component's release unit.
///
/// Two rules, and they are the same rule from two sides: a caller may move a
/// component to a new *version*, and to nothing else.
///
/// # Errors
///
/// [`Rejected`](PlatformGitError::Rejected) if the roles do not match what the
/// manifest declares, or if any role's repository does.
pub(super) fn check_release_unit(
    component: &str,
    entry: &Component,
    wanted: &ComponentVersion,
) -> Result<(), PlatformGitError> {
    let declared: Vec<&String> = entry.desired.images.keys().collect();
    let offered: Vec<&String> = wanted.images.keys().collect();

    if declared != offered {
        return Err(PlatformGitError::Rejected {
            detail: format!(
                "{component} publishes {declared:?} and they move together; the request carries {offered:?}"
            ),
        });
    }

    for (role, image) in &entry.desired.images {
        let Some(offered) = wanted.images.get(role) else {
            continue;
        };

        // A version change may not become a registry change. Where a component
        // is published is the platform repository's statement, not a caller's.
        if offered.repository != image.repository {
            return Err(PlatformGitError::Rejected {
                detail: format!(
                    "{component}/{role} is published to '{}', and the request names '{}'",
                    image.repository, offered.repository
                ),
            });
        }
    }

    Ok(())
}

/// Reads every file a pin lives in and rewrites it.
///
/// Grouped by path, because two roles can share one overlay — the control
/// plane and the console are pinned in the same kustomization. Read once,
/// rewritten twice, and written back as one change; two changes to one path
/// would be two entries in a tree, and the second would silently win.
pub(super) async fn rewrite_pins(
    repository: &PlatformGitRepository,
    head: &CommitRevision,
    entry: &Component,
    wanted: &ComponentVersion,
    roots: &[String],
) -> Result<Vec<FileChange>, PlatformGitError> {
    let mut edited: BTreeMap<String, FileChange> = BTreeMap::new();

    for (role, image) in &entry.desired.images {
        let Some(offered) = wanted.images.get(role) else {
            continue;
        };

        for path in &image.pinned_in {
            check_writable(path, roots)?;

            if !edited.contains_key(path) {
                let stored = repository.read(path, head).await?;
                edited.insert(
                    path.clone(),
                    FileChange {
                        path: path.clone(),
                        text: stored.text,
                        expected: Some(stored.revision),
                    },
                );
            }

            let change = edited
                .get_mut(path)
                .ok_or_else(|| PlatformGitError::Unavailable {
                    detail: format!("{path} was lost while being rewritten"),
                })?;

            change.text = repin(&change.text, &image.repository, &wanted.version, &offered.digest)?;
        }
    }

    Ok(edited.into_values().collect())
}

/// Records the new version in the manifest entry.
///
/// Only the version, the source commit and each image's digest. The channel,
/// the update policy, the hold and every `pinnedIn` are the platform
/// repository's, and survive untouched.
pub(super) fn apply(entry: &mut Component, wanted: &ComponentVersion) {
    entry.desired.version.clone_from(&wanted.version);
    entry.desired.source_revision.clone_from(&wanted.source_revision);

    for (role, image) in &mut entry.desired.images {
        if let Some(offered) = wanted.images.get(role) {
            image.digest.clone_from(&offered.digest);
        }
    }
}
