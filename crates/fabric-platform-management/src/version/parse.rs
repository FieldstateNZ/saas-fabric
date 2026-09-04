//! Reading a version, for an image tag and for a chart.

use crate::Version;

impl Version {
    /// Parses a version, rejecting build metadata.
    ///
    /// `+` is not a legal character in an OCI tag, so a version carrying one
    /// could never name its own image. A chart is not an image and may carry
    /// one — see [`parse_chart`](Self::parse_chart).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let parsed = Self::parse_chart(text)?;

        parsed.build.is_none().then_some(parsed)
    }

    /// Parses a version as a chart repository may publish it.
    ///
    /// The same grammar with build metadata allowed, because Helm permits it
    /// and Argo pins whatever string the index carries. It takes no part in
    /// precedence, which is why two versions differing only in build metadata
    /// are indistinguishable to a selector — and why an index offering both is
    /// refused rather than chosen between.
    #[must_use]
    pub fn parse_chart(text: &str) -> Option<Self> {
        let (text, build) = match text.split_once('+') {
            Some((rest, build)) => {
                if build.is_empty()
                    || !build
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
                {
                    return None;
                }
                (rest, Some(build.to_owned()))
            }
            None => (text, None),
        };

        let (core_text, pre_text) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };

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
                if parts.iter().any(|part| !legal_prerelease_identifier(part)) {
                    return None;
                }
                parts
            }
        };

        Some(Self {
            // As written, build metadata included: what a pin carries has to
            // be what the index published.
            text: match &build {
                Some(build) => format!("{text}+{build}"),
                None => text.to_owned(),
            },
            core,
            pre,
            build,
        })
    }
}

/// A version-core component: digits, and no leading zero unless it is zero.
fn numeric(text: &str) -> Option<u64> {
    if text.is_empty() || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }

    text.parse().ok()
}

/// Whether a prerelease identifier is legal under `SemVer` §9.
///
/// Any non-empty run of `[0-9A-Za-z-]` is legal on its own, but a *purely
/// numeric* identifier is stricter: `SemVer` forbids a leading zero, because
/// otherwise `01` and `1` would be two different spellings of the same
/// number. This also refuses a numeric identifier too large for a `u64` —
/// not a `SemVer` rule, but load-bearing here: `key()` in `ordering.rs`
/// parses a numeric identifier with `parse::<u64>()` and silently falls back
/// to comparing it as a string when that fails. An oversized identifier
/// would then sort as text instead of a number — a wrong precedence given
/// with no warning, which is the exact failure this type exists to prevent.
/// Refusing it at parse time means an identifier `key()` sees as numeric-
/// looking is always one it can actually parse as a number.
fn legal_prerelease_identifier(part: &str) -> bool {
    if part.is_empty() || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }

    if !part.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    (part.len() == 1 || !part.starts_with('0')) && part.parse::<u64>().is_ok()
}
