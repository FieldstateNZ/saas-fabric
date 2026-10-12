//! The trail's files: appending a line, recording a head, reading a stream.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use fabric_control_plane::{AuditEntry, AuditError, AuditStream, ChainHead, StoredStream};

/// The extension of a stream's log.
pub(super) const LOG_EXTENSION: &str = "jsonl";

/// The extension of a stream's head.
pub(super) const HEAD_EXTENSION: &str = "head";

/// Where `stream`'s entries live.
pub(super) fn log_path(dir: &Path, stream: &AuditStream) -> PathBuf {
    dir.join(format!("{stream}.{LOG_EXTENSION}"))
}

/// Where `stream`'s head lives.
pub(super) fn head_path(dir: &Path, stream: &AuditStream) -> PathBuf {
    dir.join(format!("{stream}.{HEAD_EXTENSION}"))
}

/// Appends `line` and a newline in one write, then flushes it to disk.
///
/// Opened for append, so the operating system places the write at the end
/// whatever any other handle did; nothing here ever seeks or truncates.
pub(super) fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(format!("{line}\n").as_bytes())?;
    file.sync_data()
}

/// Replaces `stream`'s head: written beside it, flushed, then renamed over,
/// so a reader sees the old head or the new one and never half of either.
/// The staging file's name starts with `.`, which opening skips, so one left
/// by a stop mid-write is not mistaken for a stream.
pub(super) fn write_head(dir: &Path, stream: &AuditStream, head: &ChainHead) -> std::io::Result<()> {
    let path = head_path(dir, stream);
    let next = dir.join(format!(".{stream}.{HEAD_EXTENSION}.next"));
    let text = serde_json::to_string(head).map_err(std::io::Error::other)?;
    let mut file = std::fs::File::create(&next)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&next, &path)
}

/// Reads `stream`'s entries and head, without verifying them.
///
/// # Errors
///
/// [`AuditError::Malformed`] for a line that is not an entry — including a
/// last line with no newline, which is a write that did not finish and is
/// reported rather than skipped. [`AuditError::Unavailable`] if a file could
/// not be read.
pub(super) fn read_stream(dir: &Path, stream: &AuditStream) -> Result<StoredStream, AuditError> {
    let text = read_optional(&log_path(dir, stream))?.unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        return Err(AuditError::Malformed {
            detail: format!("the last line of stream {stream} is incomplete"),
        });
    }
    let entries = text
        .lines()
        .map(AuditEntry::decode)
        .collect::<Result<Vec<_>, _>>()?;
    let anchor = read_optional(&head_path(dir, stream))?
        .map(|text| {
            serde_json::from_str::<ChainHead>(&text).map_err(|error| AuditError::Malformed {
                detail: format!("the head of stream {stream}: {error}"),
            })
        })
        .transpose()?;

    Ok(StoredStream {
        stream: stream.clone(),
        entries,
        anchor,
    })
}

/// A file's text, or `None` if it does not exist.
fn read_optional(path: &Path) -> Result<Option<String>, AuditError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AuditError::Unavailable {
            detail: format!("could not read {}: {error}", path.display()),
        }),
    }
}
