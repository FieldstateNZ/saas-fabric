//! Running the recovery command against a last-known-good directory.

use fabric_tenant_runtime::{LastKnownGoodStore, Verdict};
use serde_json::json;

use super::Action;

/// What to print, and the exit status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// One line of JSON for stdout.
    pub report: String,
    /// The process exit status.
    pub status: u8,
}

/// Runs `action`. `at_unix_seconds` stamps a discard's audit record; it is
/// passed in so the command's only clock read is in `main`.
pub async fn run(action: Action, at_unix_seconds: u64) -> Outcome {
    match action {
        Action::Inspect { directory } => {
            let inspection = LastKnownGoodStore::new(directory).inspect().await;
            outcome(&json!({ "action": "inspect", "inspection": inspection }), 0)
        }
        Action::Validate { directory } => {
            let inspection = LastKnownGoodStore::new(directory).inspect().await;
            let invalid = !matches!(inspection.verdict, Verdict::Valid { .. });
            outcome(
                &json!({ "action": "validate", "inspection": inspection }),
                u8::from(invalid),
            )
        }
        Action::Discard { directory, request } => {
            let store = LastKnownGoodStore::new(directory);
            match store.discard(&request, at_unix_seconds).await {
                Ok(Some(discarded)) => outcome(&json!({ "action": "discard", "discarded": discarded }), 0),
                Ok(None) => outcome(
                    &json!({ "action": "discard", "discarded": null, "path": store.path().display().to_string() }),
                    1,
                ),
                Err(error) => outcome(&json!({ "action": "discard", "error": error.to_string() }), 2),
            }
        }
    }
}

fn outcome(report: &serde_json::Value, status: u8) -> Outcome {
    Outcome {
        report: report.to_string(),
        status,
    }
}
