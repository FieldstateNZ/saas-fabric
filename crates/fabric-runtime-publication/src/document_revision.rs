//! The revision stamped on a published document as a whole.

use std::fmt;

/// The revision of one published document — `tenants.json`,
/// `data-sources.json`, or `catalog.json` — as recorded in its sidecar
/// [`DocumentManifest`](crate::DocumentManifest).
///
/// # This is a document's revision, never a resource's
///
/// Every tenant binding and every DataSource already carries its own
/// [`fabric_core::BindingRevision`], advanced independently by whoever edits
/// that one resource. `DocumentRevision` is a different number entirely: it
/// is the version of the *file*, stated by the caller on every publish call,
/// and it is what the publisher's monotonic and divergent-payload guards
/// compare against the manifest already on disk (a future slice's job, not
/// this one's).
///
/// Conflating the two would be a real hazard in both directions: a single
/// tenant's edit could look like it changed the whole document, or a real
/// document change could hide behind resource revisions that happen not to
/// have moved. Keeping them as separate types, never interchangeable, is
/// what stops that confusion from being representable.
///
/// Ordered, because the whole of what a caller needs to know about a
/// document revision is whether it moves the file forward.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct DocumentRevision(u64);

impl DocumentRevision {
    /// The revision of a document that has never been published.
    pub const ZERO: Self = Self(0);

    /// Wraps a raw revision number, supplied by whoever calls the publisher.
    #[must_use]
    pub const fn new(revision: u64) -> Self {
        Self(revision)
    }

    /// Returns the raw revision number, for the manifest and for telemetry.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for DocumentRevision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revisions_order_by_their_numeric_value() {
        assert!(DocumentRevision::new(9) < DocumentRevision::new(10));
    }

    #[test]
    fn zero_is_the_floor_for_a_never_published_document() {
        assert_eq!(DocumentRevision::default(), DocumentRevision::ZERO);
        assert!(DocumentRevision::ZERO < DocumentRevision::new(1));
    }
}
