//! Moving one chart source's `targetRevision` in an Argo Application.

mod entry;
mod lines;
mod position;
mod scalar;
mod seen;
mod value;
mod walk;

use entry::Entry;
use lines::Line;

use crate::PlatformGitError;

/// Why an Application is one this cannot edit, as a clause with no subject.
///
/// The modules below say what was wrong; this file says which chart and which
/// repository it was wrong about. Naming in one place is what makes every
/// refusal sound like the same renderer — and `retarget` is not told the path,
/// so that identity is all it has to offer.
pub(super) type Refusal = String;

/// Rewrites the `targetRevision` of the source naming this chart.
///
/// # Which source, and why the match is on two things
///
/// An Argo Application that deploys a chart normally has **two** sources: the
/// chart, and the repository holding its values, which carries a
/// `targetRevision` of its own that this must never touch.
///
/// ```text
/// sources:
///   - repoURL: https://codecentric.github.io/helm-charts
///     chart: keycloakx
///     targetRevision: 7.3.0          <- this one
///   - repoURL: https://github.com/…/saas-fabric-platform.git
///     targetRevision: PLACEHOLDER    <- never this one
/// ```
///
/// Matching on the chart name alone would be enough today, because only one
/// source names a chart. It is not enough as a *rule*: two sources could name
/// charts of the same name from different repositories, and the difference
/// between them is which software gets deployed. So both halves of the
/// identity the manifest declares must match, and the trusted values come from
/// `pinnedIn`, not from the file being edited.
///
/// The match is also structural, not textual. The list must be the `sources:`
/// directly under the document's top-level `spec:`, the entry must be one of
/// its own, and the revision must be that entry's own key — see
/// [`position`] and [`entry`] for what each of those rules out.
///
/// # Edited as lines, not parsed and re-serialised
///
/// The same reason the Kustomize renderer is: these files are written by
/// people and their comments are load-bearing. A load-and-dump would delete
/// every one of them and attach the damage to a routine version bump.
///
/// Byte preservation is held to the same standard: the only bytes that differ
/// between the file that goes in and the file that comes out are the version
/// token itself. Line endings, the final newline or its absence, indentation,
/// the spacing after the colon, the author's quoting and any trailing comment
/// all come back untouched.
///
/// # Errors
///
/// Returns [`PlatformGitError::Rejected`] unless **exactly one** source matches
/// and declares exactly one revision of its own. None means the manifest's
/// `pinnedIn` is wrong about this file; more than one means the file is
/// ambiguous, and choosing between them is how the wrong chart gets deployed. A
/// shape this cannot read is refused for the same reason, not guessed at.
pub(crate) fn retarget(
    text: &str,
    repository: &str,
    chart: &str,
    version: &str,
) -> Result<String, PlatformGitError> {
    let refuse = |why: Refusal| PlatformGitError::Rejected {
        detail: format!("the Argo Application pinning chart '{chart}' from {repository} {why}"),
    };

    let lines = Line::split(text).map_err(&refuse)?;
    let sources = walk::collect(&lines).map_err(&refuse)?;

    if let Some(key) = sources.iter().find_map(Entry::said_twice) {
        return Err(refuse(format!(
            "has a source declaring '{key}' twice, and which one it means cannot be told"
        )));
    }

    let matched: Vec<&Entry<'_>> = sources.iter().filter(|s| s.names(repository, chart)).collect();
    let [source] = matched.as_slice() else {
        return Err(PlatformGitError::Rejected {
            detail: match matched.len() {
                0 => format!("no source names chart '{chart}' from {repository}"),
                found => format!("{found} sources name chart '{chart}' from {repository}"),
            },
        });
    };

    let Some((pinned, revision)) = source.target() else {
        return Err(refuse(
            "names it in a source that declares no targetRevision of its own".to_owned(),
        ));
    };
    let rewritten = revision.rewrite(version).map_err(&refuse)?;

    let mut out = String::with_capacity(text.len() + version.len());
    for (index, line) in lines.iter().enumerate() {
        let content = if index == *pinned {
            rewritten.as_str()
        } else {
            line.content
        };
        out.push_str(content);
        out.push_str(line.terminator);
    }

    Ok(out)
}
