//! Which of a repository's tags are versions, newest first.

use fabric_component::ComponentVersion;
use fabric_platform_management::Version;

/// A repository's version tags, newest first, and how many tags were not
/// versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VersionTags {
    /// Tags that are component versions, by `SemVer` precedence, newest
    /// first.
    pub(crate) tags: Vec<String>,

    /// How many tags are not: `latest`, a branch name, a digest's referrers
    /// tag.
    pub(crate) other: usize,
}

/// Sorts `tags` into versions and the rest.
///
/// # Why a component version, and why precedence
///
/// A version a picker offers is one a component descriptor could describe,
/// so it is held to [`ComponentVersion`]'s rule — no `v` prefix, no build
/// metadata, one spelling per release. And it is ordered by `SemVer`
/// precedence, never as text, which would put `preview.9` after
/// `preview.10`.
pub(crate) fn sorted(tags: Vec<String>) -> VersionTags {
    let total = tags.len();
    let mut versions: Vec<Version> = tags
        .into_iter()
        .filter(|tag| ComponentVersion::try_new(tag).is_ok())
        .filter_map(|tag| Version::parse(&tag))
        .collect();
    versions.sort_by(|left, right| right.cmp(left));
    versions.dedup_by(|left, right| left.as_str() == right.as_str());

    let other = total.saturating_sub(versions.len());
    VersionTags {
        tags: versions
            .iter()
            .map(|version| version.as_str().to_owned())
            .collect(),
        other,
    }
}
