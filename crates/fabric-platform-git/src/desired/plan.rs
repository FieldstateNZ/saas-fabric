//! Working out what a version change means for the files that carry it.

use std::collections::BTreeMap;

use crate::components::{check_writable, Artifact, Component};
use crate::desired::{render::render, WantedVersion};
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
    wanted: &WantedVersion,
) -> Result<(), PlatformGitError> {
    // Only the image shape has a set of roles to agree about. A chart is one
    // artifact, so there is nothing here for it to disagree with.
    let (Artifact::Oci { images, .. }, WantedVersion::Images(wanted)) = (&entry.artifact, wanted) else {
        return Ok(());
    };
    let declared: Vec<&String> = images.keys().collect();
    let offered: Vec<&String> = wanted.images.keys().collect();

    if declared != offered {
        return Err(PlatformGitError::Rejected {
            detail: format!(
                "{component} publishes {declared:?} and they move together; the request carries {offered:?}"
            ),
        });
    }

    for (role, image) in images {
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
    component: &str,
    entry: &Component,
    wanted: &WantedVersion,
    roots: &[String],
) -> Result<Vec<FileChange>, PlatformGitError> {
    let mut edited: BTreeMap<&str, FileChange> = BTreeMap::new();

    for pin in &entry.pinned_in {
        let path = pin.path();
        check_writable(path, roots)?;

        if !edited.contains_key(path) {
            let stored = repository.read(path, head).await?;
            edited.insert(
                path,
                FileChange {
                    path: path.to_owned(),
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

        // Every arm knows exactly what it edits, and a renderer that does not
        // match the artifact is a manifest disagreeing with itself rather than
        // something to render approximately.
        if let Some(text) = render(&change.text, path, component, entry, pin, wanted)? {
            change.text = text;
        }
    }

    Ok(edited.into_values().collect())
}

/// Records the new version in the manifest entry.
///
/// Only the version, the source commit and each image's digest. The channel,
/// the update policy, the hold and every `pinnedIn` are the platform
/// repository's, and survive untouched.
pub(super) fn apply(entry: &mut Component, wanted: &WantedVersion) {
    wanted.version().clone_into(&mut entry.desired.version);

    let (
        Artifact::Oci {
            source_revision,
            images,
        },
        WantedVersion::Images(unit),
    ) = (&mut entry.artifact, wanted)
    else {
        // A chart's version is the whole of its desired state. There is no
        // provenance to record and no digest to move.
        return;
    };

    source_revision.clone_from(&unit.source_revision);

    for (role, image) in images {
        if let Some(offered) = unit.images.get(role) {
            image.digest.clone_from(&offered.digest);
        }
    }
}
