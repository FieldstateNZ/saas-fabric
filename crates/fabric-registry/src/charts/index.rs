//! The versions of one chart, read out of an index that may hold others.
//!
//! The document is parsed by [`seed`], which reads only the requested
//! chart's own raw entries without materialising anything else in the index
//! — see that module's docs for why that is load-bearing rather than just
//! tidy. What is left here is turning those entries into versions: parsing
//! each one, and refusing a duplicate rather than picking between two
//! statements of the same release.

mod seed;

use fabric_platform_management::{RegistryError, Version};

/// Every version of `chart` inside a chart index document `body`, in the
/// order the index listed them.
///
/// # Errors
///
/// [`RegistryError::Refused`] if `body` is not a YAML mapping, if `entries`
/// is not itself a mapping or appears more than once, if the requested
/// chart's key appears under `entries` more than once, if the requested
/// chart's own entries do not match their expected shape, if one of its
/// versions cannot be parsed, or if two of its versions have equal `SemVer`
/// precedence (see [`Version`]'s docs on why that is refused rather than
/// chosen between). A malformed entry under any *other* chart's name never
/// reaches any of these checks.
pub(super) fn versions_of(body: &str, chart: &str) -> Result<Vec<Version>, RegistryError> {
    let releases = seed::entries_of(body, chart).map_err(|error| RegistryError::Refused {
        detail: format!("reading a chart index: {error}"),
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
