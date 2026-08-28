//! Checking a connector implements a specification version we can talk to.

use crate::NDC_MINIMUM_VERSION;

/// [`NDC_MINIMUM_VERSION`] as numbers, for comparison.
///
/// Held separately rather than parsed at runtime: parsing a constant we wrote
/// ourselves is a fallible path with no reachable failure, and the only ways
/// to discharge that `Result` are a panic this codebase forbids or an error
/// branch no test can reach. The test at the bottom of this file pins the two
/// together, so the duplication cannot drift.
const FLOOR: (u32, u32, u32) = (0, 2, 4);

/// What checking a connector's version against ours found.
///
/// A plain `Result<(), String>` would collapse "matched exactly" and "matched
/// well enough to warn about" into the same success value, which leaves the
/// caller unable to tell them apart without re-deriving the comparison itself.
/// Naming both outcomes keeps the warning's trigger a fact the type carries,
/// not a side effect buried inside [`check_version`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VersionOutcome {
    /// The connector implements exactly [`NDC_MINIMUM_VERSION`].
    Matched,

    /// Same major and minor, ahead of our floor on the patch. Accepted, and
    /// worth a warning so the drift is visible to an operator.
    AheadOfFloor {
        /// The version the connector reported.
        connector_version: String,
    },
}

/// Checks the connector's specification version against our floor.
///
/// # Why a floor, and not just a matching minor
///
/// The wire format is **not** stable within a minor version, and this function
/// used to claim it was. 0.2.x added protocol surface at patch level
/// repeatedly: request-level arguments in 0.2.4, an `Interval` scalar in
/// 0.2.6, `from_type` on casts in 0.2.8, `string_agg_with_separator` in
/// 0.2.10. Only one of those matters to this client, and it is the one the
/// entire multi-tenant design rests on. A connector implementing 0.2.0 to
/// 0.2.3 has never heard of `request_arguments` — and because `ndc-models`
/// declares no `deny_unknown_fields` at any version, it does not *reject* the
/// field, it ignores it, and serves every tenant from whichever connection it
/// was configured with while answering `200`.
///
/// # Why the floor is 0.2.4 and not 0.2.13
///
/// 0.2.4 is the first version carrying the feature this client depends on, and
/// nothing above it is used here — the additions from 0.2.5 onward are all
/// relational-query and aggregate surface the Data API never asks for. That is
/// also exactly what `versioning.md` asks a client to advertise: "the minimum
/// non-breaking version of the specification that is supported by the client,
/// so that the widest range of connectors can be used".
///
/// The header carries the same value, which is the point. `versioning.md`
/// defines compatibility as `^{requested-version}`, so sending `0.2.13` while
/// accepting `0.2.0` advertised a contract this function did not honour in
/// either direction — too strict on paper, far too loose in fact. Matching the
/// two is the fix, and it is what lets a real connector through: `ndc-postgres`
/// v3.1.0 pins `ndc-models` at v0.2.4 and reports `0.2.4`.
///
/// # What is still fatal
///
/// A differing **major or minor**, in either direction. Our wire types are
/// hand-written against one specification, so a connector speaking a different
/// minor may serialise fields we do not read or expect fields we do not send,
/// and that failure would show up as malformed responses under load rather
/// than as a clear error at boot.
///
/// A **patch below the floor**, for the reason above.
///
/// A version that is not three numeric components. It is rejected rather than
/// defaulted to anything, so an unparseable version can never pass silently.
///
/// # Errors
///
/// A message naming both versions and what this client requires.
pub(super) fn check_version(connector: &str, connector_version: &str) -> Result<VersionOutcome, String> {
    let (floor_major, floor_minor, floor_patch) = FLOOR;

    let Some((major, minor, patch)) = parse(connector_version) else {
        return Err(format!(
            "connector {connector} reports NDC version {connector_version}, which is not a \
             major.minor.patch version; this client requires {floor_major}.{floor_minor}.x at \
             {NDC_MINIMUM_VERSION} or later"
        ));
    };

    if (major, minor) != (floor_major, floor_minor) || patch < floor_patch {
        return Err(format!(
            "connector {connector} implements NDC {connector_version}, but this client requires \
             {floor_major}.{floor_minor}.x at {NDC_MINIMUM_VERSION} or later; request-level \
             arguments, which carry every tenant's connection routing, were only added in \
             {NDC_MINIMUM_VERSION}"
        ));
    }

    if connector_version == NDC_MINIMUM_VERSION {
        return Ok(VersionOutcome::Matched);
    }

    Ok(VersionOutcome::AheadOfFloor {
        connector_version: connector_version.to_owned(),
    })
}

/// Parses `major.minor.patch` — all three components, all three numeric.
///
/// Stricter than the previous `major.minor` split, which treated an
/// unparseable version as its own opaque value. That was safe but imprecise:
/// with a patch-level floor to enforce, a version without a patch component is
/// not a version this client can reason about, so it is refused rather than
/// guessed at.
fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.');

    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );

    parts.next().is_none().then_some(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_constant_agrees_with_the_version_we_advertise() {
        // The one thing keeping the two representations of the floor honest.
        assert_eq!(parse(NDC_MINIMUM_VERSION), Some(FLOOR));
    }

    #[test]
    fn parses_a_three_component_version() {
        assert_eq!(parse("0.2.13"), Some((0, 2, 13)));
    }

    #[test]
    fn refuses_a_version_without_a_patch_component() {
        assert_eq!(parse("0.2"), None);
    }

    #[test]
    fn refuses_a_version_with_trailing_components() {
        assert_eq!(parse("0.2.13.1"), None);
    }

    #[test]
    fn refuses_a_non_numeric_component() {
        assert_eq!(parse("0.2.13-rc.1"), None);
        assert_eq!(parse("nonsense"), None);
    }
}
