//! Moving one `images:` entry in a Kustomize overlay.

use crate::PlatformGitError;

/// Rewrites the entry naming `repository` to carry `version` and `digest`.
///
/// # Edited as lines, not parsed and re-serialised
///
/// These overlays are written by people, and their comments are the reason a
/// pin is what it is — which listener a route selects, why a replica count is
/// zero. A load-and-dump would delete every one of them, and the diff would
/// look like an unrelated rewrite attached to a version bump.
///
/// So the entry is found by its `- name:` line and its indented keys are
/// replaced in place. Everything else in the file, including the comment above
/// the block, is emitted unchanged.
///
/// Both a tag and a digest are written. Kustomize accepts both and emits
/// `repository:tag@sha256:...`: the digest is what Kubernetes resolves, so the
/// artifact cannot move under the pin, and the tag keeps the manifest readable
/// by somebody who wants to know which version they are looking at.
///
/// # Errors
///
/// Returns [`PlatformGitError::Rejected`] unless exactly one entry names that
/// repository. None means the manifest's `pinnedIn` is wrong about this file;
/// more than one means the file is ambiguous, and guessing between them is how
/// half a promotion lands.
pub(crate) fn repin(
    text: &str,
    repository: &str,
    version: &str,
    digest: &str,
) -> Result<String, PlatformGitError> {
    let marker = format!("- name: {repository}");
    let mut out = String::with_capacity(text.len() + 96);
    let mut lines = text.lines().peekable();
    let mut rewritten = 0_usize;

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed != marker {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        let indent = &line[..line.len() - trimmed.len()];
        out.push_str(line);
        out.push('\n');

        // The entry's own keys are indented two further than its `- `. A
        // following entry starts with `- ` at the same indent as this one, so
        // it is never consumed.
        let continuation = format!("{indent}  ");
        while lines.peek().is_some_and(|next| {
            next.starts_with(&continuation) && !next[continuation.len()..].starts_with('-')
        }) {
            lines.next();
        }

        for (key, value) in [("newTag", version), ("digest", digest)] {
            out.push_str(&continuation);
            out.push_str(key);
            out.push_str(": ");
            out.push_str(value);
            out.push('\n');
        }
        rewritten += 1;
    }

    if rewritten != 1 {
        return Err(PlatformGitError::Rejected {
            detail: format!("expected exactly one '{repository}' pin to rewrite, found {rewritten}"),
        });
    }

    Ok(out)
}
