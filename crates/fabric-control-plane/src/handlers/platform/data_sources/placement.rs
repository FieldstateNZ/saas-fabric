//! Translating a placement class between the console's word and the wire's.
//!
//! Every other field in a declaration is spelled exactly as the wire spells
//! it (ADR 0023 part 1): the entry is built from and rendered through the
//! wire's own sub-types, so a hand edit cannot disagree with what gets
//! published from it. `placement` is the one exception, because the wire's
//! `PlacementClassDocument` is `snake_case` — right for `data-sources.yaml`,
//! beside `components.yaml` — and every other body this API answers is
//! `camelCase` throughout. Translating it here, in one place with an
//! exhaustive match either way, is what keeps that exception from leaking
//! into every handler that touches a declaration.

#[cfg(test)]
#[path = "placement_tests.rs"]
mod placement_tests;

use fabric_platform_management::PlacementClassDocument;

use crate::extraction::malformed;
use crate::ControlPlaneError;

/// The console's word for a placement class read from a declaration.
///
/// Exhaustive rather than a fallback default: if the wire ever grows a
/// seventh placement class, this fails to compile rather than silently
/// rendering nothing for it.
pub(super) const fn console_word(placement: PlacementClassDocument) -> &'static str {
    match placement {
        PlacementClassDocument::Shared => "shared",
        PlacementClassDocument::Dedicated => "dedicated",
        PlacementClassDocument::HighAvailability => "highAvailability",
        PlacementClassDocument::Regulated => "regulated",
        PlacementClassDocument::Development => "development",
        PlacementClassDocument::Ephemeral => "ephemeral",
    }
}

/// Parses the console's word for a placement class, or refuses it as a
/// malformed request.
///
/// # Errors
///
/// [`ControlPlaneError::InvalidRequest`] naming the closed set of words this
/// platform accepts, when `word` is not one of them.
pub(super) fn parse_word(word: &str) -> Result<PlacementClassDocument, ControlPlaneError> {
    match word {
        "shared" => Ok(PlacementClassDocument::Shared),
        "dedicated" => Ok(PlacementClassDocument::Dedicated),
        "highAvailability" => Ok(PlacementClassDocument::HighAvailability),
        "regulated" => Ok(PlacementClassDocument::Regulated),
        "development" => Ok(PlacementClassDocument::Development),
        "ephemeral" => Ok(PlacementClassDocument::Ephemeral),
        other => Err(malformed(format!(
            "placement must be one of shared, dedicated, highAvailability, regulated, development, \
             ephemeral, not \"{other}\""
        ))),
    }
}
