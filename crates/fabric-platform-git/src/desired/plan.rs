//! Working out what a version change means for the files that carry it.

use std::collections::BTreeMap;

use crate::components::{check_writable, Component};
use crate::desired::{render::render, WantedVersion};
use crate::host::PlatformGitRepository;
use crate::{CommitRevision, FileChange, PlatformGitError};

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
