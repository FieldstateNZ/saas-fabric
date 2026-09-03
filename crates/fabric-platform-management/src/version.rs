//! Versions, and the ordering that keeps an environment moving forwards.

mod ordering;
mod parse;

#[cfg(test)]
mod version_tests;

/// Which release stream a version belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Prereleases: integration candidates, and never a cloud environment's.
    Preview,

    /// Released versions, carrying no prerelease part.
    Stable,
}

/// A `SemVer` version, ordered by `SemVer` precedence.
///
/// # Precedence, not string order
///
/// This is the whole reason the type exists. `preview.9` precedes
/// `preview.10`, and a string comparison has it the other way round — so a
/// "never move backwards" rule built on `<` over strings would wave through
/// exactly the rollback it exists to prevent, and only once a preview number
/// reached two digits.
/// # Equality is precedence, not spelling
///
/// `PartialEq` is written rather than derived so that it agrees with `Ord`.
/// `SemVer` says build metadata is ignored when comparing, so `1.2.3+a` and
/// `1.2.3+b` order as equal — and a derived equality over the text would call
/// them different, which is the kind of disagreement that makes a `BTreeSet`
/// quietly keep both or quietly keep one.
///
/// Two versions that compare equal and are spelled differently are a problem
/// for whoever is choosing between them, and the chart index refuses such a
/// pair rather than picking.
#[derive(Debug, Clone, Eq)]
pub struct Version {
    /// The version as written, which is also the image tag.
    pub(crate) text: String,

    /// Major, minor, patch.
    pub(crate) core: (u64, u64, u64),

    /// Dot-separated prerelease identifiers, empty for a release.
    pub(crate) pre: Vec<String>,

    /// Build metadata, if the version carried any.
    ///
    /// Kept because it is part of how the version is *written* — a chart
    /// pinned as `1.2.3+build.7` has to be written back that way — and ignored
    /// in every comparison, because `SemVer` says it is not part of precedence.
    pub(crate) build: Option<String>,
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Version {
    /// The version as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The channel this version belongs to.
    #[must_use]
    pub fn channel(&self) -> Channel {
        if self.pre.is_empty() {
            Channel::Stable
        } else {
            Channel::Preview
        }
    }

    /// Whether this is a version of the same `major.minor.patch` line.
    #[must_use]
    pub fn is_series(&self, series: &Self) -> bool {
        self.core == series.core
    }
}
