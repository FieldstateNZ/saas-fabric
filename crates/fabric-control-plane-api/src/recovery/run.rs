//! Running the recovery command against a publication target.

use fabric_publication_kubernetes::{KubernetesRuntimePublication, PublicationTarget};
use fabric_runtime_publication::{diagnose, FilesystemRuntimePublication, HeldRecovery, RecoveryError};
use serde_json::json;

use super::audit::{check_apart, AuditLog};
use super::{Action, Target};

/// What to print, and the exit status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// One line of JSON for stdout.
    pub report: String,
    /// The process exit status.
    pub status: u8,
}

/// Runs `action`. `at_unix_seconds` stamps a restore's audit record; it is
/// passed in so the command's only clock read is in `main`.
pub async fn run(action: Action, at_unix_seconds: u64) -> Outcome {
    match action {
        Action::Inspect { target } => report(&target, "inspect", false).await,
        Action::Validate { target } => report(&target, "validate", true).await,
        Action::Restore {
            target,
            document,
            from,
            audit,
            operator,
            reason,
        } => {
            if let Target::Filesystem {
                tenants,
                data_sources,
                catalog,
            } = &target
            {
                if let Err(error) = check_apart(&audit, &[tenants, data_sources, catalog]) {
                    return failure("restore", &error);
                }
            }
            let offered = match std::fs::read(&from) {
                Ok(bytes) => bytes,
                Err(error) => return failure("restore", &format!("--from cannot be read: {error}")),
            };
            let adapter = match adapter(&target) {
                Ok(adapter) => adapter,
                Err(error) => return failure("restore", &error),
            };
            let log = AuditLog {
                path: audit,
                operator,
                reason,
                target: adapter.describe(),
                from: from.display().to_string(),
                at_unix_seconds,
            };
            match adapter.restore(document, &offered, &log).await {
                Ok(plan) => outcome(&json!({ "action": "restore", "restored": plan }), 0),
                Err(RecoveryError::Refused(refusal)) => {
                    outcome(&json!({ "action": "restore", "refused": refusal.to_string() }), 1)
                }
                Err(error) => failure("restore", &error.to_string()),
            }
        }
    }
}

async fn report(target: &Target, action: &str, strict: bool) -> Outcome {
    let adapter = match adapter(target) {
        Ok(adapter) => adapter,
        Err(error) => return failure(action, &error),
    };
    match adapter.observe().await {
        Ok(observation) => {
            let diagnosis = diagnose(&observation);
            let status = u8::from(strict && !diagnosis.blocked.is_empty());
            outcome(
                &json!({ "action": action, "target": adapter.describe(), "diagnosis": diagnosis }),
                status,
            )
        }
        Err(error) => failure(action, &error.to_string()),
    }
}

fn adapter(target: &Target) -> Result<Box<dyn HeldRecovery>, String> {
    match target {
        Target::Filesystem {
            tenants,
            data_sources,
            catalog,
        } => Ok(Box::new(FilesystemRuntimePublication::new(
            tenants,
            data_sources,
            catalog,
        ))),
        Target::Kubernetes { namespace } => KubernetesRuntimePublication::in_cluster(PublicationTarget {
            namespace: namespace.clone(),
        })
        .map(|adapter| Box::new(adapter) as Box<dyn HeldRecovery>),
    }
}

fn failure(action: &str, error: &str) -> Outcome {
    outcome(&json!({ "action": action, "error": error }), 2)
}

fn outcome(report: &serde_json::Value, status: u8) -> Outcome {
    Outcome {
        report: report.to_string(),
        status,
    }
}
