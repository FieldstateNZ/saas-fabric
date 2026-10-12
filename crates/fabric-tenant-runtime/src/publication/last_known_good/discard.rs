//! Discarding the copy: an explicit, audited operator action.

use std::io;
use std::path::Path;

use tokio::io::AsyncWriteExt as _;

use super::atomic_replace::sync_directory;
use super::inspection::{Inspection, Verdict};
use super::store::FILE_NAME;
use super::LastKnownGoodStore;

/// The audit log beside the copy: one JSON object per line, appended and
/// synced before the action it records.
pub const AUDIT_FILE_NAME: &str = "audit.jsonl";

/// Who is discarding the copy, and why. Both are required and recorded.
#[derive(Debug, Clone)]
pub struct DiscardRequest {
    /// The operator, as they identify themselves.
    pub operator: String,
    /// Why the copy is being discarded.
    pub reason: String,
}

/// What a discard did.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Discarded {
    /// The copy as it was before it was moved aside.
    pub inspection: Inspection,
    /// Where the copy now is. It is moved aside, not deleted, so an
    /// incident can still examine it.
    pub moved_to: String,
}

impl LastKnownGoodStore {
    /// Moves the copy aside so no reader serves it, after appending an
    /// audit record of who, why, when and exactly which bytes.
    ///
    /// Valid and refused copies alike may be discarded. Nothing is written
    /// in the copy's place: a replica that then restarts with no complete
    /// published generation fails closed rather than serving anything this
    /// command made up. `Ok(None)` when there was no copy; nothing is
    /// audited then, because nothing changed.
    ///
    /// # Errors
    ///
    /// [`io::Error`] if the audit record cannot be written (the copy is then
    /// left in place) or the copy cannot be moved (a failure line is
    /// appended to the audit log).
    pub async fn discard(
        &self,
        request: &DiscardRequest,
        at_unix_seconds: u64,
    ) -> io::Result<Option<Discarded>> {
        let inspection = self.inspect().await;
        if inspection.verdict == Verdict::Absent {
            return Ok(None);
        }
        let fingerprint: String = inspection
            .file_sha256
            .as_deref()
            .unwrap_or("unread")
            .chars()
            .take(12)
            .collect();
        let moved_to = self
            .path()
            .with_file_name(format!("{FILE_NAME}.discarded-{at_unix_seconds}-{fingerprint}"));
        let audit = self.path().with_file_name(AUDIT_FILE_NAME);
        let record = serde_json::json!({
            "at_unix_seconds": at_unix_seconds,
            "action": "discard",
            "operator": request.operator,
            "reason": request.reason,
            "inspection": inspection,
            "moved_to": moved_to.display().to_string(),
        });
        append(&audit, &record).await?;
        if let Err(error) = move_aside(self.path(), &moved_to).await {
            let failed = serde_json::json!({
                "at_unix_seconds": at_unix_seconds,
                "action": "discard_failed",
                "operator": request.operator,
                "error": error.to_string(),
            });
            let _ = append(&audit, &failed).await;
            return Err(error);
        }
        Ok(Some(Discarded {
            inspection,
            moved_to: moved_to.display().to_string(),
        }))
    }
}

async fn move_aside(from: &Path, to: &Path) -> io::Result<()> {
    tokio::fs::rename(from, to).await?;
    sync_directory(from).await
}

async fn append(audit: &Path, record: &serde_json::Value) -> io::Result<()> {
    let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
    line.push(b'\n');
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(audit)
        .await?;
    file.write_all(&line).await?;
    file.sync_all().await
}
