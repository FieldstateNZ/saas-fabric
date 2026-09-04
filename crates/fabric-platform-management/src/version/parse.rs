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

/// Whether `text` is a canonical run of digits: non-empty, all ASCII
/// digits, and no leading zero unless it is exactly `0`.
///
/// `SemVer` requires this both for a version-core component (§2) and a
/// purely numeric prerelease identifier (§9). One function is the single
/// source of truth for it: `numeric` calls it before parsing, and
/// `legal_prerelease_identifier` calls it directly, since an identifier is
/// never parsed as a number — `SemVer` puts no bound on its size.
fn is_canonical_digits(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| c.is_ascii_digit())
        && (text.len() == 1 || !text.starts_with('0'))
}

/// A version-core component: digits, and no leading zero unless it is zero.
fn numeric(text: &str) -> Option<u64> {
    if !is_canonical_digits(text) {
        return None;
    }

    text.parse().ok()
}

/// Whether a prerelease identifier is legal under `SemVer` §9.
///
/// Any non-empty run of `[0-9A-Za-z-]` is legal on its own; a *purely
/// numeric* one is stricter, via the same canonical-digits rule as a
/// version-core component. It is never parsed as a number — `SemVer` puts
/// no bound on one, and `ordering.rs` compares numeric identifiers by digit
/// count and text instead.
fn legal_prerelease_identifier(part: &str) -> bool {
    if part.is_empty() || !part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return false;
    }

    if part.chars().all(|c| c.is_ascii_digit()) {
        return is_canonical_digits(part);
    }

    true
}
