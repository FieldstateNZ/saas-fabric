//! The restore's audit lines: the operation's intent before any write,
//! then how it ended. Operator text is self-identification, not authority.

use std::io;

use fabric_runtime_publication::{RecoveryAudit, RestorePlan};
use serde_json::json;

use super::audit_file::AuditFile;

/// Who restores, why, and where the operation's lines go.
pub(super) struct AuditLog {
    pub(super) operation_id: String,
    pub(super) operator: String,
    pub(super) reason: String,
    pub(super) target: String,
    pub(super) from: String,
    pub(super) at_unix_seconds: u64,
    pub(super) log: AuditFile,
}

impl AuditLog {
    /// Records how the operation ended: `completed`, `failed` (nothing was
    /// replaced) or `outcome_uncertain` (inspect again).
    pub(super) fn outcome(&self, outcome: &str, error: Option<&str>) -> io::Result<()> {
        self.log.append(&json!({
            "operation_id": self.operation_id,
            "at_unix_seconds": self.at_unix_seconds,
            "action": format!("restore_{outcome}"),
            "error": error,
        }))
    }
}

impl RecoveryAudit for AuditLog {
    fn intent(&self, plan: &RestorePlan) -> io::Result<()> {
        self.log.append(&json!({
            "operation_id": self.operation_id,
            "at_unix_seconds": self.at_unix_seconds,
            "action": "restore_intent",
            "operator_self_identified": self.operator,
            "reason": self.reason,
            "target": self.target,
            "from": self.from,
            "plan": plan,
        }))
    }
}
