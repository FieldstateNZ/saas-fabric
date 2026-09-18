//! Whether an environment's held declarations are still something to trust.

#[cfg(test)]
#[path = "held_tests.rs"]
mod held_tests;

use std::collections::BTreeSet;

use crate::data_sources::declaration::DataSourceDeclaration;
use crate::PlatformError;

/// Refuses a held document that a hand edit has made incoherent: two
/// entries claiming one id, or an entry that no longer satisfies its own
/// rules.
///
/// # Why this runs on every read, not only when writing
///
/// Break-glass edits keep working by design (ADR 0023 part 1), which means
/// a file this crate did not write can reach here. A `list` that returned
/// it uncritically would show an operator a document with two conflicting
/// entries for one id as though it were fine, and a `declare` that planned
/// against it would inherit whichever quiet assumption the plan happened
/// to make. Refusing here, once, is cheaper than auditing every reader for
/// the same care.
///
/// # Errors
///
/// [`PlatformError::InvalidHeldDataSources`], naming the id and, for an
/// invalid entry, the rule it broke -- in the platform's own words, never
/// a path. Its own variant rather than `DesiredStateError::Refused`,
/// which an adapter also answers for a reason that is not a coherence
/// problem in the document -- see that variant's rustdoc.
pub(crate) fn check_held(declarations: &[DataSourceDeclaration]) -> Result<(), PlatformError> {
    let mut seen = BTreeSet::new();

    for declared in declarations {
        if !seen.insert(&declared.id) {
            return Err(PlatformError::InvalidHeldDataSources {
                detail: format!("{} is declared more than once", declared.id),
            });
        }

        if let Err(rule) = declared.validate() {
            return Err(PlatformError::InvalidHeldDataSources {
                detail: format!("{} is invalid: {rule}", declared.id),
            });
        }
    }

    Ok(())
}
