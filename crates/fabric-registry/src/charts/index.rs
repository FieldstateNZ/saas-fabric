//! The versions of one chart, read out of an index that may hold others.
//!
//! # Only the requested chart's shape is trusted
//!
//! A chart repository serves every chart it holds in one document. The
//! obvious shape to deserialise `entries` into is `BTreeMap<String,
//! Vec<Entry>>` — but that forces *every* chart's releases through the
//! requested chart's strict shape before this can even look at the one it
//! was asked for. A neighbour with a scalar entry, a missing `version`, or a
//! `version` that is a list rather than a string would then make the
//! requested chart undiscoverable, for a reason that has nothing to do with
//! it.
//!
//! So `entries` is read one level looser first, as `BTreeMap<String,
//! serde_norway::Value>`, and only the value under the requested chart's own
//! name is deserialised into [`Entry`]. A `serde::de::DeserializeSeed` that
//! ignored every other key while parsing would avoid holding the rest of the
//! document as `Value` at all, but the streamed read this module is handed
//! already bounds what that tree can cost, and the seed is meaningfully more
//! code to carry for a saving that never shows up on a repository this
//! platform actually reads. The simpler shape wins.

use fabric_platform_management::{RegistryError, Version};

/// Just enough of an index to find one chart's releases.
///
/// Everything but the requested chart's own entries stays an opaque
/// `serde_norway::Value` — see the module docs for why.
#[derive(serde::Deserialize)]
struct Index {
    /// Releases by chart name, not yet trusted to hold any particular shape.
    #[serde(default)]
    entries: std::collections::BTreeMap<String, serde_norway::Value>,
}

/// One published release of a chart.
#[derive(serde::Deserialize)]
struct Entry {
    /// The chart version, which is what Argo pins.
    version: String,
}

/// Every version of `chart` inside a chart index document `body`, in the
/// order the index listed them.
///
/// # Errors
///
/// [`RegistryError::Refused`] if `body` is not a YAML mapping, if `entries`
/// is not itself a mapping, if the requested chart's own entries do not
/// match [`Entry`]'s shape, if one of its versions cannot be parsed, or if
/// two of its versions have equal `SemVer` precedence (see [`Version`]'s
/// docs on why that is refused rather than chosen between). A malformed
/// entry under any *other* chart's name never reaches any of these checks.
pub(super) fn versions_of(body: &str, chart: &str) -> Result<Vec<Version>, RegistryError> {
    let mut index: Index = serde_norway::from_str(body).map_err(|error| RegistryError::Refused {
        detail: format!("reading a chart index: {error}"),
    })?;

    // Only this chart's own releases, removed rather than cloned. Another
    // chart's malformed entry never reaches `Entry`'s deserialisation at
    // all, because it is never given the chance to.
    let Some(value) = index.entries.remove(chart) else {
        return Ok(Vec::new());
    };

    let releases: Vec<Entry> = serde_norway::from_value(value).map_err(|error| RegistryError::Refused {
        detail: format!("{chart}'s entries in the chart index: {error}"),
    })?;

    let mut seen = std::collections::BTreeSet::new();
    let mut versions = Vec::with_capacity(releases.len());

    for entry in releases {
        // Refused, not skipped. A version this cannot read is one it cannot
        // order either, so skipping it would mean answering "the newest is
        // X" while holding something that might have been newer -- a wrong
        // answer given confidently, which is worse than none.
        let version = Version::parse_chart(&entry.version).ok_or_else(|| RegistryError::Refused {
            detail: format!("{chart} lists '{}', which is not a version", entry.version),
        })?;

        // `BTreeSet::insert` is the O(log n) check `Vec::contains` was doing
        // in O(n): `Version`'s `Ord` is `SemVer` precedence, so two spellings
        // differing only in build metadata land on the same key, and the
        // second insert reports it was already there -- exactly the
        // collision this refuses.
        if !seen.insert(version.clone()) {
            return Err(RegistryError::Refused {
                detail: format!("{chart} lists {version} more than once"),
            });
        }

        versions.push(version);
    }

    Ok(versions)
}
