//! An append-only audit trail in a local directory: one JSON-lines file per
//! stream, each beside the head it was last extended to.
//!
//! # Why a file, not Git
//!
//! Desired state is persisted through a Git host's contents API, which
//! rewrites a whole file on every write and cannot read one back past about
//! 1 MB — the very limit the activity feed runs into (ADR 0021). A trail
//! stored that way would be rewritten, not appended, and would hit the same
//! ceiling. Commits are also authored by the platform's machine identity and
//! a refused operation leaves none. A file opened for append, one entry per
//! line, flushed before an append returns, is append-only at the storage
//! layer too. The local repository (`local_repository`) sets the precedent:
//! a development store in a directory, refused to a second process by an OS
//! lock. Where the trail lives in a live environment is deferred (ADR 0028).
//!
//! # Layout
//!
//! ```text
//! <dir>/.audit.lock          held while the trail is open
//! <dir>/<stream>.jsonl       one sealed entry per line, never rewritten
//! <dir>/<stream>.head        the last appended entry's sequence and hash
//! ```

mod errors;
mod files;
mod open;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use fabric_control_plane::{
    AuditEntry, AuditError, AuditRecord, AuditStream, AuditTrail, ChainHead, StoredStream,
};

pub use errors::LocalAuditTrailError;

/// The local development [`AuditTrail`]. See this module's rustdoc.
///
/// Every append runs whole on a blocking thread under one lock: read the
/// head, seal, write and flush the line, then record the head. A caller
/// dropping its future cannot stop that part-way, so the in-memory head can
/// never fall behind the file and fork the chain.
pub struct LocalAuditTrail {
    inner: Arc<Inner>,
    _lock: std::fs::File,
}

/// What every append and read works on.
struct Inner {
    dir: PathBuf,
    state: Mutex<Heads>,
}

/// Each stream's head, and the streams a failed write left unsafe to extend.
#[derive(Default)]
struct Heads {
    heads: BTreeMap<AuditStream, ChainHead>,
    broken: BTreeSet<AuditStream>,
}

#[async_trait]
impl AuditTrail for LocalAuditTrail {
    async fn append(&self, record: AuditRecord, at: u64) -> Result<AuditEntry, AuditError> {
        let inner = Arc::clone(&self.inner);
        blocking(move || inner.append(record, at)).await
    }

    async fn read(&self, stream: &AuditStream) -> Result<StoredStream, AuditError> {
        let dir = self.inner.dir.clone();
        let stream = stream.clone();
        blocking(move || files::read_stream(&dir, &stream)).await
    }

    fn describe(&self) -> String {
        format!("a local development audit trail at {}", self.inner.dir.display())
    }
}

impl Inner {
    /// One append, start to finish. See [`LocalAuditTrail`].
    fn append(&self, record: AuditRecord, at: u64) -> Result<AuditEntry, AuditError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| unavailable("the audit trail lock is poisoned"))?;
        let stream = record.stream().clone();
        if state.broken.contains(&stream) {
            return Err(unavailable(
                "an earlier write to this stream failed; it needs inspection",
            ));
        }
        let entry = AuditEntry::seal(state.heads.get(&stream), at, record)?;
        let line = entry.encode()?;

        if let Err(error) = files::append_line(&files::log_path(&self.dir, &stream), &line) {
            state.broken.insert(stream);
            return Err(unavailable(&error.to_string()));
        }
        state.heads.insert(stream.clone(), entry.head());

        // The line is durable, so the entry is recorded. A head that failed
        // to write is caught up on the next open: an entry past its head is
        // a complete record (`verify`).
        if let Err(error) = files::write_head(&self.dir, &stream, &entry.head()) {
            tracing::warn!(
                event = "control_plane.audit.head_not_written",
                stream = %stream,
                error = %error,
                "an audit entry was appended but its head was not recorded"
            );
        }
        Ok(entry)
    }
}

/// Runs `work` on a blocking thread, as a task of its own.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, AuditError> + Send + 'static,
) -> Result<T, AuditError> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|_| Err(unavailable("the audit trail operation did not complete")))
}

/// An [`AuditError::Unavailable`] carrying `detail`.
fn unavailable(detail: &str) -> AuditError {
    AuditError::Unavailable {
        detail: detail.to_owned(),
    }
}
