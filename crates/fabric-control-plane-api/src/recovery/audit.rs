//! The restore's audit log: one JSON object per line, appended and synced
//! before the write it records.

use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use fabric_runtime_publication::{PublicationError, RecoveryAudit, RestorePlan};
use serde_json::json;

/// Who is restoring, why, when, and where the log goes.
pub(super) struct AuditLog {
    pub(super) path: PathBuf,
    pub(super) operator: String,
    pub(super) reason: String,
    pub(super) target: String,
    pub(super) from: String,
    pub(super) at_unix_seconds: u64,
}

impl RecoveryAudit for AuditLog {
    fn before(&self, plan: &RestorePlan) -> io::Result<()> {
        append(
            &self.path,
            &json!({
                "at_unix_seconds": self.at_unix_seconds,
                "action": "restore",
                "operator": self.operator,
                "reason": self.reason,
                "target": self.target,
                "from": self.from,
                "plan": plan,
            }),
        )
    }

    fn failed(&self, plan: &RestorePlan, error: &PublicationError) {
        let _ = append(
            &self.path,
            &json!({
                "at_unix_seconds": self.at_unix_seconds,
                "action": "restore_failed",
                "operator": self.operator,
                "document": plan.document,
                "error": error.to_string(),
            }),
        );
    }
}

fn append(path: &Path, record: &serde_json::Value) -> io::Result<()> {
    let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
    line.push(b'\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&line)?;
    file.sync_all()
}

/// Refuses an audit log in a directory a published document is in: on the
/// filesystem it would sit beside what readers mount, and the publisher
/// alone writes there.
pub(super) fn check_apart(audit: &Path, published: &[&Path]) -> Result<(), String> {
    let directory = |path: &Path| {
        let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty());
        parent.unwrap_or_else(|| Path::new(".")).canonicalize()
    };
    let audit_directory =
        directory(audit).map_err(|error| format!("the audit log's directory cannot be used: {error}"))?;
    for path in published {
        if directory(path).is_ok_and(|directory| directory == audit_directory) {
            return Err(
                "the audit log may not be in a published document's directory; give one apart from them"
                    .to_owned(),
            );
        }
    }
    Ok(())
}
