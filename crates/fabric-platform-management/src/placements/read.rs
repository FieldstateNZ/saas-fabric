//! What an environment has recorded placed, and the revision it was read at.

use crate::placements::record::PlacementRecord;
use crate::DesiredRevision;

/// Every placement an environment has recorded, and the revision it was
/// read at.
///
/// The revision is `Option` at this port for the same reason
/// [`DataSourcesRead`](crate::DataSourcesRead)'s is: an adapter's truth
/// about the file is genuinely optional at that layer --
/// [`PlacementState::read_placements`](crate::PlacementState::read_placements)
/// answers `None` when no file exists yet, not per placement. The late-bound
/// binding this platform runs always fills this with `Some`, tagged with
/// its own generation, so a write built from this read carries proof it was
/// decided against the repository currently bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementsRead {
    /// `None` only at the adapter port, when no file exists yet for the
    /// environment asked about. The binding this platform runs always
    /// fills this with `Some`.
    pub revision: Option<DesiredRevision>,

    /// Every recorded placement, sorted by (tenant, logical).
    pub placements: Vec<PlacementRecord>,
}
