//! Which generation a reader with a last-known-good copy applies.
//!
//! Pure, so every hold and fail-closed rule is a unit test away.

use crate::publication::complete_generation::CompleteGeneration;
use crate::SourceError;

/// What one read decided.
#[derive(Debug)]
pub(crate) enum Choice {
    /// Apply the published generation.
    Published(CompleteGeneration),
    /// Apply the persisted copy, because no published generation at or
    /// above it could be applied.
    LastKnownGood {
        generation: CompleteGeneration,
        held_because: String,
    },
    /// No record, and nothing recorded was ever held: read each document on
    /// its own, as a reader without the record does.
    Unrecorded,
    /// Apply nothing. At start with no valid copy this fails the prime
    /// (fail closed); on a refresh the replica keeps what it serves.
    Refused(SourceError),
}

/// The choice at process start, between what the published files offer and
/// the validated copy, if any.
///
/// - A published generation at or above the copy wins: roll forward only.
/// - A copy above a valid published generation is held: a generation never
///   goes back (D01-1), so a lower one is a lost or reverted volume.
/// - No record, or a published read that fails, holds the copy.
/// - With no valid copy, a failed published read is refused: fail closed.
pub(crate) fn at_start(
    published: Result<Option<CompleteGeneration>, SourceError>,
    copy: Option<CompleteGeneration>,
) -> Choice {
    match (published, copy) {
        (Ok(Some(published)), Some(copy)) if copy.generation > published.generation => {
            Choice::LastKnownGood {
                held_because: format!(
                    "the published generation {} is below the last-known-good generation {}",
                    published.generation, copy.generation
                ),
                generation: copy,
            }
        }
        (Ok(Some(published)), _) => Choice::Published(published),
        (Ok(None), Some(copy)) => Choice::LastKnownGood {
            generation: copy,
            held_because: "the published files carry no publication record".to_owned(),
        },
        (Ok(None), None) => Choice::Unrecorded,
        (Err(error), Some(copy)) => Choice::LastKnownGood {
            generation: copy,
            held_because: error.to_string(),
        },
        (Err(error), None) => Choice::Refused(error),
    }
}

/// The choice on a refresh, once this replica holds `floor`: the highest
/// generation it has served or persisted, if any.
pub(crate) fn on_refresh(
    published: Result<Option<CompleteGeneration>, SourceError>,
    floor: Option<u64>,
    origin: &str,
) -> Choice {
    let regressed = |held: u64, found: String| {
        Choice::Refused(SourceError::Regressed {
            origin: origin.to_owned(),
            held,
            found,
        })
    };
    match (published, floor) {
        (Ok(Some(published)), Some(held)) if published.generation < held => {
            regressed(held, format!("generation {}", published.generation))
        }
        (Ok(Some(published)), _) => Choice::Published(published),
        (Ok(None), Some(held)) => regressed(held, "no publication record".to_owned()),
        (Ok(None), None) => Choice::Unrecorded,
        (Err(error), _) => Choice::Refused(error),
    }
}
