//! Moving one chart source's `targetRevision` in an Argo Application.

mod position;

use position::Position;

use crate::PlatformGitError;

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
/// # Edited as lines, not parsed and re-serialised
///
/// The same reason the Kustomize renderer is: these files are written by
/// people and their comments are load-bearing. A load-and-dump would delete
/// every one of them and attach the damage to a routine version bump.
///
/// # Errors
///
/// Returns [`PlatformGitError::Rejected`] unless **exactly one** source
/// matches. None means the manifest's `pinnedIn` is wrong about this file;
/// more than one means the file is ambiguous, and choosing between them is how
/// the wrong chart gets deployed.
pub(crate) fn retarget(
    text: &str,
    repository: &str,
    chart: &str,
    version: &str,
) -> Result<String, PlatformGitError> {
    let mut out = String::with_capacity(text.len() + 32);
    let mut rewritten = 0_usize;
    let mut source: Option<Source> = None;
    let mut sources = Position::Outside;

    for line in text.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        // Where in the document this line is. Only entries of `spec.sources`
        // are sources; a `- ` anywhere else is somebody else's list, and a
        // renderer that edited one because it happened to carry a `chart:` key
        // would be exactly the arbitrary-edit engine this design refuses.
        sources.observe(trimmed, indent);

        let (starts_entry, key) = match trimmed.strip_prefix("- ") {
            Some(rest) => (true, rest),
            None => (false, trimmed),
        };

        if starts_entry {
            source = sources.entry_at(indent).then(Source::default);
        }

        // Leaving the list closes whatever source was open, so a key after
        // it is never read as part of one.
        if !sources.inside() {
            source = None;
        }

        if let Some(open) = source.as_mut() {
            open.observe(key);

            if open.matches(repository, chart) && key.starts_with("targetRevision:") {
                out.push_str(&line[..line.len() - key.len()]);
                out.push_str("targetRevision: ");
                out.push_str(version);
                out.push('\n');
                rewritten += 1;
                continue;
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    match rewritten {
        1 => Ok(out),
        0 => Err(PlatformGitError::Rejected {
            detail: format!("no source names chart '{chart}' from {repository}"),
        }),
        found => Err(PlatformGitError::Rejected {
            detail: format!("{found} sources name chart '{chart}' from {repository}"),
        }),
    }
}

/// What has been seen of the source currently being read.
#[derive(Default)]
struct Source {
    /// Its `repoURL`, if it has declared one yet.
    repository: Option<String>,

    /// Its `chart`, if it has declared one yet.
    chart: Option<String>,
}

impl Source {
    /// Records a key of this source.
    ///
    /// `targetRevision` may appear before `repoURL` or after it, so a source is
    /// only ever matched on what has been read so far — which is why the file
    /// is walked twice in effect: once to accumulate, once to decide. Argo's
    /// own manifests put `repoURL` first, and a file that does not is one this
    /// refuses rather than half-edits, because `rewritten` would then be zero.
    fn observe(&mut self, key: &str) {
        for (name, field) in [("repoURL:", &mut self.repository), ("chart:", &mut self.chart)] {
            if let Some(value) = key.strip_prefix(name) {
                *field = Some(value.trim().trim_matches(['"', '\'']).to_owned());
            }
        }
    }

    /// Whether this is the source the manifest declared.
    fn matches(&self, repository: &str, chart: &str) -> bool {
        self.repository.as_deref() == Some(repository) && self.chart.as_deref() == Some(chart)
    }
}
