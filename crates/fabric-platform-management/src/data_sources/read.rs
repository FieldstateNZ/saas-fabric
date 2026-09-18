//! What an environment declares, and the revision it was read at.

use crate::data_sources::declaration::DataSourceDeclaration;
use crate::DesiredRevision;

/// What an environment declares, and the revision it was read at.
///
/// The revision is `Option` at this port because an adapter's truth about
/// a file is genuinely optional at that layer: `DataSourceState::read_data_sources`
/// answers `None` when no file exists yet for the environment it was asked
/// about, per document rather than per id -- a file with three declared
/// data sources and one with none are the only two shapes an adapter
/// reports. The binding this platform runs (`PlatformDesiredState`) always
/// fills this with `Some` before anything above it sees a read: it tags
/// even an absent file with a generation, so there is always something to
/// compare a write against, and a write built from this read must present
/// that tag back so a correction decided against state that has since
/// moved is refused rather than applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourcesRead {
    /// `None` only at the adapter port, when no file exists yet for the
    /// environment asked about. The binding this platform runs always
    /// fills this with `Some`.
    pub revision: Option<DesiredRevision>,

    /// Every declared data source, sorted by id.
    pub declarations: Vec<DataSourceDeclaration>,
}
