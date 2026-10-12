//! The trail's files: appending a line, recording a head, reading a stream.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use fabric_control_plane::{AuditEntry, AuditError, AuditStream, ChainHead, StoredStream};

use super::durability::Durability;

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

/// Appends `line` and a newline in one write, then flushes it to storage.
///
/// Opened for append, so the operating system places the write at the end
/// whatever any other handle did; nothing here ever seeks or truncates. The
/// write that creates a stream also syncs the directory: until its name is
/// durable, neither is the entry.
pub(super) fn append_line(
    dir: &Path,
    stream: &AuditStream,
    line: &str,
    durability: &dyn Durability,
) -> std::io::Result<()> {
    let path = log_path(dir, stream);
    let mut options = std::fs::OpenOptions::new();
    options.append(true);
    let (mut file, created) = match options.clone().create_new(true).open(&path) {
        Ok(file) => (file, true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (options.open(&path)?, false),
        Err(error) => return Err(error),
    };
    file.write_all(format!("{line}\n").as_bytes())?;
    durability.sync_file(&file, &path)?;
    if created {
        durability.sync_dir(dir)?;
    }
    Ok(())
}

/// Replaces `stream`'s head: written beside it, flushed, renamed over, then
/// the directory synced, so a reader sees the old head or the new one and
/// never half of either. The staging file's name starts with `.`, which
/// opening skips, so one left by a stop mid-write is not mistaken for a
/// stream.
pub(super) fn write_head(
    dir: &Path,
    stream: &AuditStream,
    head: &ChainHead,
    durability: &dyn Durability,
) -> std::io::Result<()> {
    let next = dir.join(format!(".{stream}.{HEAD_EXTENSION}.next"));
    let text = serde_json::to_string(head).map_err(std::io::Error::other)?;
    let mut file = std::fs::File::create(&next)?;
    file.write_all(text.as_bytes())?;
    durability.sync_file(&file, &next)?;
    std::fs::rename(&next, head_path(dir, stream))?;
    durability.sync_dir(dir)
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
