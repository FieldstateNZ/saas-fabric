//! Restoring one document on the filesystem: the source and audit log are
//! checked apart from the target before anything is written, the adapter
//! records the intent, and the outcome is recorded after.

use std::path::{Path, PathBuf};

use fabric_runtime_publication::{DocumentKind, FilesystemRuntimePublication, HeldRecovery, RecoveryError};
use serde_json::json;

use super::audit::AuditLog;
use super::audit_file::AuditFile;
use super::identity::{identities, identity, published_files, read_source};
use super::run::{failure, outcome, Outcome};

/// One restore, as asked for.
pub(super) struct Restore {
    pub(super) document: DocumentKind,
    pub(super) from: PathBuf,
    pub(super) audit: PathBuf,
    pub(super) operator: String,
    pub(super) reason: String,
    pub(super) operation_id: String,
    pub(super) at_unix_seconds: u64,
}

pub(super) async fn restore(
    tenants: &Path,
    data_sources: &Path,
    catalog: &Path,
    request: Restore,
) -> Outcome {
    let published = published_files(tenants, data_sources, catalog);
    let published_identities = identities(&published);
    let (offered, source) = match read_source(&request.from, &published_identities) {
        Ok(read) => read,
        Err(error) => return failure("restore", &error),
    };
    let directories: Vec<PathBuf> = [tenants, data_sources, catalog]
        .iter()
        .map(|path| path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf())
        .collect();
    let source_identity = match source.metadata() {
        Ok(metadata) => identity(&metadata),
        Err(error) => return failure("restore", &format!("--from cannot be examined: {error}")),
    };
    let forbidden = [published_identities, vec![source_identity]].concat();
    let log = match AuditFile::open(&request.audit, &directories, &forbidden) {
        Ok(log) => log,
        Err(error) => return failure("restore", &error),
    };
    let adapter = FilesystemRuntimePublication::new(tenants, data_sources, catalog);
    let audit = AuditLog {
        operation_id: request.operation_id.clone(),
        operator: request.operator,
        reason: request.reason,
        target: HeldRecovery::describe(&adapter),
        from: request.from.display().to_string(),
        at_unix_seconds: request.at_unix_seconds,
        log,
        published,
        source,
    };
    let operation_id = request.operation_id;
    match adapter.restore(request.document, &offered, &audit).await {
        Ok(plan) if plan.already_restored() => outcome(
            &json!({ "action": "restore", "already_restored": true, "restored": plan }),
            0,
        ),
        Ok(plan) => match audit.outcome("completed", None) {
            Ok(()) => outcome(
                &json!({ "action": "restore", "operation_id": operation_id, "restored": plan }),
                0,
            ),
            Err(error) => outcome(
                &json!({ "action": "restore", "operation_id": operation_id, "restored": plan,
                         "outcome_not_audited": error.to_string() }),
                2,
            ),
        },
        Err(RecoveryError::Refused(refusal)) => {
            outcome(&json!({ "action": "restore", "refused": refusal.to_string() }), 1)
        }
        Err(error @ (RecoveryError::Target(_) | RecoveryError::Audit(_))) => {
            failure("restore", &error.to_string())
        }
        Err(error) => {
            let (word, outcome_word) = match error {
                RecoveryError::Uncertain(_) => ("outcome_uncertain", "uncertain"),
                _ => ("failed", "failed"),
            };
            let audited = audit.outcome(word, Some(&error.to_string()));
            outcome(
                &json!({ "action": "restore", "operation_id": operation_id, "outcome": outcome_word,
                         "error": error.to_string(), "outcome_audited": audited.is_ok(),
                         "next": "inspect the target again before anything else" }),
                2,
            )
        }
    }
}

#[cfg(test)]
#[path = "restore_tests.rs"]
mod tests;
