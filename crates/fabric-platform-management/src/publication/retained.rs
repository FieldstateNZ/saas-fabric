//! What a restore checks a tombstoned client's placements against: the held
//! publication itself, read from the target, and the O4 guard that refuses
//! a removed, replaced or lowered placement (D01-14 restore).

use fabric_runtime_publication::{PublicationError, PublishedResources, TenantBindingDocument};

use super::compose_error::ComposeError;
use super::RuntimePublisher;
use crate::PlacementRecord;

impl RuntimePublisher {
    /// What the publication target holds now: each tenant's binding and the
    /// resource history raised to cover it. Read from the target, never from
    /// this process's memory of a pass, so a restart loses nothing.
    ///
    /// # Errors
    ///
    /// [`PublicationError`] when the target cannot be read, or what it holds
    /// does not parse.
    pub async fn published_resources(&self) -> Result<PublishedResources, PublicationError> {
        self.target.published_resources().await
    }
}

/// Refuses `placements` that no longer account for `held` exactly as it was
/// published: a logical binding with no record, a record naming another
/// data source or isolation without raising the tenant's revision, or
/// records summing below it. The same guard every publication pass applies
/// (O4, D01-27), for the tenants a caller names.
///
/// # Errors
///
/// [`ComposeError::PlacementRemoved`] or [`ComposeError::PlacementLowered`].
pub fn placements_retained(
    placements: &[PlacementRecord],
    held: &[TenantBindingDocument],
) -> Result<(), ComposeError> {
    super::placement_history::refuse_removed_or_lowered(placements, held)
}
