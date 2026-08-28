//! What the adapter reads out of the contents API.

use fabric_client_model::ClientRevision;

/// A file, decoded.
///
/// The revision is the blob hash rather than the commit that produced it, and
/// the difference matters: a commit touching another client's document moves
/// the branch but not this file's hash, so revisions built from commits would
/// make every operator's edit conflict with every other operator's unrelated
/// edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredFile {
    /// The file's text.
    pub(crate) text: String,

    /// The blob hash, as a revision.
    pub(crate) revision: ClientRevision,
}

/// One entry in a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectoryEntry {
    /// The entry's name within its directory.
    pub(crate) name: String,

    /// Whether it is a directory.
    pub(crate) is_directory: bool,
}
