//! What a caller reads and writes, in the caller's terms.
//!
//! Nothing here is a Git Data primitive. A revision is an opaque token that
//! came from a read and goes back with a write; the adapter knows it is a blob
//! hash and the caller does not need to.

/// The revision of one file, as read.
///
/// A content hash, not a counter: it moves when *this file* changes and stays
/// put when the branch moves for any other reason. That is what lets an
/// unrelated commit to the platform repository cost a retry rather than a
/// conflict.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FileRevision(String);

impl FileRevision {
    /// Wraps a revision the host reported.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value, for comparison and for sending back.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The revision of the branch as a whole, after a write.
///
/// Returned so a caller can record *which commit* carried its change — the
/// answer to "what is this environment running, and where did that come from"
/// lives in Git history, and this is the handle to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRevision(String);

impl CommitRevision {
    /// Wraps a commit the host reported.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A file, as read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    /// Repository-relative path.
    pub path: String,

    /// The file's text.
    pub text: String,

    /// What it was when it was read.
    pub revision: FileRevision,
}

/// One file's new content, and what the caller believed it was editing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChange {
    /// Repository-relative path.
    pub path: String,

    /// The text to write.
    pub text: String,

    /// The revision the caller read, or `None` if it expected no such file.
    ///
    /// Carried per file rather than per write, because the whole point of the
    /// retry is to ask "did *these* paths move" separately from "did the
    /// branch move".
    pub expected: Option<FileRevision>,
}
