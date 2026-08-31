//! Versions, and the ordering that keeps an environment moving forwards.

mod ordering;

#[cfg(test)]
mod version_tests;

/// Which release stream a version belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The version as written, which is also the image tag.
    pub(crate) text: String,

    /// Major, minor, patch.
    pub(crate) core: (u64, u64, u64),

    /// Dot-separated prerelease identifiers, empty for a release.
    pub(crate) pre: Vec<String>,
}

impl Version {
    /// Parses a version, rejecting build metadata.
    ///
    /// `+` is not a legal character in an OCI tag, so a version carrying one
    /// could never name its own image.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (core_text, pre_text) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };

        if core_text.contains('+') || pre_text.is_some_and(|pre| pre.contains('+')) {
            return None;
        }

        let mut parts = core_text.split('.');
        let core = (
            numeric(parts.next()?)?,
            numeric(parts.next()?)?,
            numeric(parts.next()?)?,
        );
        if parts.next().is_some() {
            return None;
        }

        let pre = match pre_text {
            None => Vec::new(),
            Some(pre) => {
                let parts: Vec<String> = pre.split('.').map(ToOwned::to_owned).collect();
                if parts.iter().any(|part| {
                    part.is_empty() || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                }) {
                    return None;
                }
                parts
            }
        };

        Some(Self {
            text: text.to_owned(),
            core,
            pre,
        })
    }

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

/// A version-core component: digits, and no leading zero unless it is zero.
fn numeric(text: &str) -> Option<u64> {
    if text.is_empty() || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }

    text.parse().ok()
}
