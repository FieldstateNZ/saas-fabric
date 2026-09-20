//! One publication pass, as an operator reads it.

use fabric_platform_management::{PassOutcome, WaitingReason};

/// One publication pass, as an operator reads it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPassRow {
    /// When it finished, as seconds since the Unix epoch.
    ///
    /// Sent unformatted, exactly as `LastCheckRow::at_unix_seconds` is, so
    /// the browser renders it in the operator's own timezone.
    pub at_unix_seconds: u64,

    /// `published` | `unchanged` | `waiting` | `refused` | `failed`.
    pub outcome: &'static str,

    /// Why, when `outcome` is anything but `published` or `unchanged`.
    /// Already sanitised by [`fabric_platform_management::SafeDiagnostic`]
    /// -- never a credential, a response body, or a path.
    pub detail: Option<String>,
}

impl LastPassRow {
    /// Renders straight from an outcome and the moment it is reported at,
    /// without reading `PublicationState` at all -- the trigger handler's
    /// own read, over the outcome its own call to `publish_once` just
    /// returned, so a scheduled pass finishing in the gap between that call
    /// returning and this rendering can never make the trigger's response
    /// describe a pass the operator did not ask for.
    pub(crate) fn at(at_unix_seconds: u64, outcome: &PassOutcome) -> Self {
        let detail = match outcome {
            PassOutcome::Waiting { reason } => Some(waiting_reason_text(reason)),
            PassOutcome::Refused { reason } => Some(reason.as_str().to_owned()),
            PassOutcome::Failed { detail } => Some(detail.as_str().to_owned()),
            PassOutcome::Published { .. } | PassOutcome::Unchanged { .. } => None,
        };

        Self {
            at_unix_seconds,
            outcome: pass_outcome_word(outcome),
            detail,
        }
    }
}

/// Why a pass found nothing ready to publish, in an operator's own words.
fn waiting_reason_text(reason: &WaitingReason) -> String {
    match reason {
        WaitingReason::NoResources => "the derived runtime catalogue has no resources yet".to_owned(),
        WaitingReason::PlatformNotConnected => "no platform repository is connected yet".to_owned(),
    }
}

/// Which word an outcome renders as, shared between [`LastPassRow::at`] and
/// the trigger handler's own audit record, so the two can never disagree
/// about what a pass was called.
pub(crate) fn pass_outcome_word(outcome: &PassOutcome) -> &'static str {
    match outcome {
        PassOutcome::Published { .. } => "published",
        PassOutcome::Unchanged { .. } => "unchanged",
        PassOutcome::Waiting { .. } => "waiting",
        PassOutcome::Refused { .. } => "refused",
        PassOutcome::Failed { .. } => "failed",
    }
}
