//! The publication row: what this environment's runtime-publication target
//! holds, and what the last pass did.

mod last_pass_row;
mod readers_row;
mod readiness_row;

pub(crate) use last_pass_row::pass_outcome_word;
pub use last_pass_row::LastPassRow;
pub(crate) use readers_row::observe_readers;
pub use readers_row::ReadersRow;
pub use readiness_row::ReadinessRow;

use fabric_platform_management::{PassOutcome, PublicationState, ReaderApplication, RuntimePublisher};
use fabric_runtime_publication::{DocumentRevision, PublishedRevisions, RegistryGeneration};

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

    /// The three readiness gates, as of the last pass the target accepted.
    pub readiness: ReadinessRow,
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
    /// The issuer registry's held generation, carried with the tenants
    /// document; `null` when none is published.
    pub registry: Option<u64>,
}

impl PublicationRow {
    /// Renders this environment's publication state from what
    /// `PublicationState` currently holds -- `GET /api/platform`'s own
    /// read, where there is no fresher outcome to prefer.
    pub(crate) fn of(
        publisher: &RuntimePublisher,
        state: &PublicationState,
        readers: ReaderApplication,
    ) -> Self {
        match state.last_pass() {
            Some(pass) => Self::at(publisher, state, (pass.at_unix_seconds, &pass.outcome), readers),
            None => Self {
                target: publisher.describe_target(),
                documents: PublishedDocumentsRow::default(),
                last_pass: None,
                readiness: ReadinessRow::of(state.readiness(), readers),
            },
        }
    }

    /// Renders straight from an outcome and the moment it is reported at,
    /// without a `PublicationState` read of its own -- the trigger
    /// handler's own read, over the outcome its own call to `publish_once`
    /// just returned, so a scheduled pass finishing in the gap between that
    /// call returning and this rendering can never make the response
    /// describe a pass the operator did not ask for. See
    /// `LastPassRow::at`'s own rustdoc for the full argument.
    /// The readiness row is `state`'s: it changes only when a pass is
    /// accepted, which `publish_once` records before it returns.
    pub(crate) fn at(
        publisher: &RuntimePublisher,
        state: &PublicationState,
        (at_unix_seconds, outcome): (u64, &PassOutcome),
        readers: ReaderApplication,
    ) -> Self {
        Self {
            target: publisher.describe_target(),
            documents: PublishedDocumentsRow::of(outcome),
            last_pass: Some(LastPassRow::at(at_unix_seconds, outcome)),
            readiness: ReadinessRow::of(state.readiness(), readers),
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
            registry: revisions.registry.map(RegistryGeneration::get),
        }
    }
}
