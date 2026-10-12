//! The completeness row: whether the target held one whole recorded
//! generation when the last pass inspected it (G3).

use fabric_platform_management::CompletenessObservation;
use fabric_runtime_publication::PublicationCompleteness;

use super::readers_row::StampRow;

/// What the last pass's inspection of the target found. Kept apart from
/// the last pass's outcome: a pass can fail having written nothing and
/// leave the target complete. Rendered from what the pass recorded; a `GET`
/// never reads the target. Says nothing about what readers applied.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessRow {
    /// `complete`, `incomplete`, `unpublished` or `unobserved`. `unobserved`
    /// also before any pass has inspected the target in this process.
    pub state: &'static str,
    /// When the pass that inspected it finished; `null` before any has.
    pub observed_at_unix_seconds: Option<u64>,
    /// The record the target was inspected against, when one was read.
    pub record: Option<StampRow>,
    /// Why it is not complete, in an operator's words; `null` when it is.
    /// Names parts and the generation only, never their contents.
    pub reason: Option<String>,
}

impl CompletenessRow {
    pub(crate) fn of(observed: Option<CompletenessObservation>) -> Self {
        let reason = fabric_platform_management::incompleteness(observed.as_ref());
        let Some(observed) = observed else {
            return Self {
                state: "unobserved",
                observed_at_unix_seconds: None,
                record: None,
                reason,
            };
        };
        let (state, record) = match observed.completeness {
            PublicationCompleteness::Complete { record } => ("complete", Some(record)),
            PublicationCompleteness::Incomplete { record, .. } => ("incomplete", Some(record)),
            PublicationCompleteness::Unpublished => ("unpublished", None),
            PublicationCompleteness::Unobserved { .. } => ("unobserved", None),
        };
        Self {
            state,
            observed_at_unix_seconds: Some(observed.at_unix_seconds),
            record: record.map(|stamp| StampRow {
                generation: stamp.generation,
                record_sha256: stamp.sha256.to_string(),
            }),
            reason,
        }
    }
}
