//! The publication row: what this environment's runtime-publication target
//! holds, and what the last pass did.

use fabric_platform_management::{LastPass, PassOutcome, PublicationState, RuntimePublisher, WaitingReason};
use fabric_runtime_publication::{DocumentRevision, PublishedRevisions};

/// What an operator is told about this environment's runtime publication.
///
/// `None` on [`super::PlatformBody`] rather than a placeholder row: a
/// deployment with no publication target configured reports nothing here at
/// all, the same way [`super::PlatformBody`] reports no platform when
/// nothing is managed -- see `PlatformBinding::publisher`'s own rustdoc.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationRow {
    /// Where publication writes, safe for an operator to read --
    /// [`RuntimePublisher::describe_target`]'s own contract never returns a
    /// credential.
    pub target: String,

    /// What is currently held for each document, as of the last pass that
    /// saw it. Every field is `null` before a first pass has run, or when
    /// the last pass halted before a read of `current()` was worth
    /// reporting as "held" (`waiting`, `refused`, `failed`).
    pub documents: PublishedDocumentsRow,

    /// What the last pass did, or `null` if none has run.
    pub last_pass: Option<LastPassRow>,
}

/// The revision currently held for each of the runtime's three documents.
#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedDocumentsRow {
    /// The tenants document's held revision.
    pub tenants: Option<u64>,
    /// The data-sources document's held revision.
    pub data_sources: Option<u64>,
    /// The catalogue document's held revision.
    pub catalog: Option<u64>,
}

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

impl PublicationRow {
    /// Renders this environment's publication state.
    pub(crate) fn of(publisher: &RuntimePublisher, state: &PublicationState) -> Self {
        let last_pass = state.last_pass();

        Self {
            target: publisher.describe_target(),
            documents: last_pass
                .as_ref()
                .map_or_else(PublishedDocumentsRow::default, |pass| {
                    PublishedDocumentsRow::of(&pass.outcome)
                }),
            last_pass: last_pass.as_ref().map(LastPassRow::of),
        }
    }
}

impl PublishedDocumentsRow {
    /// The held revisions a pass saw once it reached them, or every field
    /// `None` when the pass halted before `current()`'s answer was worth
    /// reporting as "held".
    fn of(outcome: &PassOutcome) -> Self {
        match outcome {
            PassOutcome::Published { revisions, .. } | PassOutcome::Unchanged { revisions } => {
                Self::from_revisions(revisions)
            }
            PassOutcome::Waiting { .. } | PassOutcome::Refused { .. } | PassOutcome::Failed { .. } => {
                Self::default()
            }
        }
    }

    fn from_revisions(revisions: &PublishedRevisions) -> Self {
        Self {
            tenants: revisions.tenants.map(DocumentRevision::get),
            data_sources: revisions.data_sources.map(DocumentRevision::get),
            catalog: revisions.catalog.map(DocumentRevision::get),
        }
    }
}

impl LastPassRow {
    fn of(pass: &LastPass) -> Self {
        let detail = match &pass.outcome {
            PassOutcome::Waiting { reason } => Some(waiting_reason_text(reason)),
            PassOutcome::Refused { reason } => Some(reason.as_str().to_owned()),
            PassOutcome::Failed { detail } => Some(detail.as_str().to_owned()),
            PassOutcome::Published { .. } | PassOutcome::Unchanged { .. } => None,
        };

        Self {
            at_unix_seconds: pass.at_unix_seconds,
            outcome: pass_outcome_word(&pass.outcome),
            detail,
        }
    }
}

/// Why a pass found nothing ready to publish, in an operator's own words.
fn waiting_reason_text(reason: &WaitingReason) -> String {
    match reason {
        WaitingReason::NoResources => "the derived runtime catalogue has no resources yet".to_owned(),
    }
}

/// Which word an outcome renders as, shared between [`LastPassRow::of`] and
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
