//! Which files this crate is willing to write.

use std::path::Path;

use crate::PlatformGitError;

/// The file types a pin can live in.
const MANIFEST_SUFFIXES: [&str; 2] = ["yaml", "yml"];

/// Refuses a declared path this crate will not write.
///
/// # Six rules, and why a trusted file still gets them
///
/// `pinnedIn` is desired state in the repository being written to, so it is
/// not untrusted input. It is still bounded, because a mistake in it would
/// otherwise make this a confused deputy — editing `.github/workflows/`, or a
/// README, on the strength of a trusted document asking it to. The rules bound
/// the *class* of file, independently of any individual path:
///
/// ```text
/// repository-relative, never absolute
/// no traversal
/// under a managedRoot
/// a .yaml or .yml manifest
/// exists                        ← checked by the caller, which reads it
/// and actually pins the image   ← checked by the caller, which parses it
/// ```
///
/// The last two need the file's content and are enforced where it is read. The
/// platform repository's CI enforces all six as well, and that duplication is
/// deliberate: CI proves the manifest is coherent at the commit it ran on, and
/// this applies the same rules to whatever was actually read, which may be a
/// state no CI has seen.
///
/// # Errors
///
/// Returns [`PlatformGitError::Rejected`], naming the path and the rule.
pub(crate) fn check_writable(path: &str, managed_roots: &[String]) -> Result<(), PlatformGitError> {
    let refuse = |why: &str| {
        Err(PlatformGitError::Rejected {
            detail: format!("{path} {why}"),
        })
    };

    if path.is_empty() || path.starts_with('/') || path.starts_with('\\') {
        return refuse("is not a repository-relative path");
    }

    let parts = Path::new(path).components().collect::<Vec<_>>();
    if parts.iter().any(|part| {
        matches!(
            part,
            std::path::Component::ParentDir | std::path::Component::Prefix(_)
        )
    }) {
        return refuse("escapes the repository");
    }

    // Every root is checked, not just the first: a permissive entry added
    // beside a real one must not widen the others.
    let admitted = managed_roots.iter().any(|root| {
        let root = root.trim();
        !root.is_empty() && root.ends_with('/') && path.starts_with(root)
    });

    if !admitted {
        return refuse("is under no managedRoot");
    }

    let suffix = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();

    if !MANIFEST_SUFFIXES.contains(&suffix) {
        return refuse("is not a manifest");
    }

    Ok(())
}
