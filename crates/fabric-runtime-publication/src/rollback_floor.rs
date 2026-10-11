//! The publisher's rollback floor a held publication record carries
//! (D01-21), checked before anything else in the record is read.

use crate::record::unreadable;
use crate::PublicationError;

/// Only the floor. Read on its own, so a record written under a newer
/// contract is refused by its floor rather than by a field this publisher
/// has never seen.
#[derive(serde::Deserialize)]
struct Floor {
    minimum_publisher_contract: u32,
}

/// Refuses a held record that a publisher at contract `publisher` may not
/// write over.
///
/// Takes the contract as a parameter rather than reading
/// [`CONTRACT_VERSION`](crate::CONTRACT_VERSION) itself. Publishers since
/// #136 have run exactly this check against their own constant, so calling it
/// with an older contract stands in for an older binary in the format tests
/// below (`format_compatibility_tests.rs`).
///
/// # Errors
///
/// [`PublicationError::BelowRollbackFloor`] when the record's floor is above
/// `publisher`; [`PublicationError::Unreadable`] when the floor cannot be read.
pub(crate) fn check_publisher_floor(bytes: &[u8], publisher: u32) -> Result<(), PublicationError> {
    let floor: Floor = serde_json::from_slice(bytes).map_err(unreadable)?;
    if floor.minimum_publisher_contract > publisher {
        return Err(PublicationError::BelowRollbackFloor {
            floor: floor.minimum_publisher_contract,
            publisher,
        });
    }
    Ok(())
}
