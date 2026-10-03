//! Why the versions that were not selected were not, as the console reads it.

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod diagnostics_tests;

mod found;
mod state;

use fabric_platform_management::{Diagnostics, InvalidVersion, Version};

use self::found::found;
pub use self::state::DiagnosticState;

/// A version that exists and was not selected, and why.
///
/// # Every state its own word, and none falls through
///
/// The console words each state for itself (ADR 0026 section 9). An operator
/// deciding whether to wait needs *still publishing* not to read as *built
/// more than once*, and a described component's *no component descriptor
/// attached* not to read as either. So the state is a closed enum, and a
/// reason exists only on the one state that has one: a row that could carry a
/// reason beside `publishing` would be a row the console had to decide what to
/// do with.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticRow {
    /// The version.
    pub version: String,

    /// Why it was not selected, written into this row as `state` and, for
    /// `invalid` only, `reason`.
    #[serde(flatten)]
    pub state: DiagnosticState,
}

impl DiagnosticRow {
    /// Every version that was not selected, as one list: still publishing,
    /// then undescribed, then built more than once, then invalid, each in
    /// the order discovery found them.
    ///
    /// # Destructured, so nothing goes unrendered
    ///
    /// Each list is named in the pattern below rather than read field by
    /// field, so a fifth list in [`Diagnostics`] fails to compile here
    /// instead of never reaching the console. A new reason needs no change
    /// here: [`InvalidReason::code`](fabric_platform_management::InvalidReason::code)
    /// is an exhaustive match of its own.
    pub(super) fn every(diagnostics: &Diagnostics) -> Vec<Self> {
        let Diagnostics {
            not_yet,
            incoherent,
            undescribed,
            invalid,
        } = diagnostics;

        not_yet
            .iter()
            .map(|version| Self::of(version, DiagnosticState::Publishing))
            .chain(
                undescribed
                    .iter()
                    .map(|version| Self::of(version, DiagnosticState::Undescribed)),
            )
            .chain(
                incoherent
                    .iter()
                    .map(|version| Self::of(version, DiagnosticState::Incoherent)),
            )
            .chain(invalid.iter().map(|InvalidVersion { version, reason }| {
                Self::of(
                    version,
                    DiagnosticState::Invalid {
                        reason: reason.code(),
                        found: found(reason),
                    },
                )
            }))
            .collect()
    }

    /// One row.
    fn of(version: &Version, state: DiagnosticState) -> Self {
        Self {
            version: version.as_str().to_owned(),
            state,
        }
    }
}
