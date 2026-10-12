//! Running the recovery command against a publication target.

use fabric_publication_kubernetes::{KubernetesRuntimePublication, PublicationTarget};
use fabric_runtime_publication::{diagnose, FilesystemRuntimePublication, HeldRecovery, RestoreRefusal};
use serde_json::json;

use super::restore::{restore, Restore};
use super::{Action, Target};

/// What to print, and the exit status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// One line of JSON for stdout.
    pub report: String,
    /// The process exit status.
    pub status: u8,
}

/// Runs `action`. `at_unix_seconds` stamps the audit lines and, with the
/// process id, names the operation; it is passed in so the command's only
/// clock read is in `main`.
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
            let Target::Filesystem {
                tenants,
                data_sources,
                catalog,
            } = target
            else {
                let refusal = RestoreRefusal::OwnershipUnestablished.to_string();
                return outcome(&json!({ "action": "restore", "refused": refusal }), 1);
            };
            let request = Restore {
                document,
                from,
                audit,
                operator,
                reason,
                operation_id: format!("restore-{at_unix_seconds}-{}", std::process::id()),
                at_unix_seconds,
            };
            restore(&tenants, &data_sources, &catalog, request).await
        }
    }
}

async fn report(target: &Target, action: &str, strict: bool) -> Outcome {
    let adapter: Box<dyn HeldRecovery> = match target {
        Target::Filesystem {
            tenants,
            data_sources,
            catalog,
        } => Box::new(FilesystemRuntimePublication::new(tenants, data_sources, catalog)),
        Target::Kubernetes { namespace } => {
            match KubernetesRuntimePublication::in_cluster(PublicationTarget {
                namespace: namespace.clone(),
            }) {
                Ok(adapter) => Box::new(adapter),
                Err(error) => return failure(action, &error),
            }
        }
    };
    match adapter.observe().await {
        Ok(held) => {
            let diagnosis = diagnose(&held);
            let status = u8::from(strict && !diagnosis.clear);
            outcome(
                &json!({ "action": action, "target": adapter.describe(), "diagnosis": diagnosis }),
                status,
            )
        }
        Err(error) => failure(action, &error.to_string()),
    }
}

pub(super) fn failure(action: &str, error: &str) -> Outcome {
    outcome(&json!({ "action": action, "error": error }), 2)
}

pub(super) fn outcome(report: &serde_json::Value, status: u8) -> Outcome {
    Outcome {
        report: report.to_string(),
        status,
    }
}
