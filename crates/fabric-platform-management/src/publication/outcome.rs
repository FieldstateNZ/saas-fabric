//! What one publication pass did, or why it did not do anything.

use fabric_runtime_publication::{DocumentOutcome, PublishedRevisions};

use crate::SafeDiagnostic;

/// What a publication pass found and did, once it has run.
///
/// Mirrors [`crate::CheckOutcome`]'s shape, for the same reason: a console
/// asking "did the last pass publish" needs one variant per honestly
/// distinct answer, not a boolean plus a string that swallows the
/// difference between "refused" and "failed". Five, not two, because an
/// operator acts on each differently -- `Waiting` means "nothing to do
/// yet, come back later"; `Refused` means "something needs a human
/// decision"; `Failed` means "try again, this was a transport problem".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassOutcome {
    /// At least one document was rewritten.
    Published {
        /// What happened to the tenants document.
        tenants: DocumentOutcome,
        /// What happened to the data-sources document.
        data_sources: DocumentOutcome,
        /// What happened to the catalogue document.
        catalog: DocumentOutcome,
        /// What is now held for every document, once this pass finished.
        revisions: PublishedRevisions,
    },

    /// Every document already matched what this pass offered. Nothing was
    /// written, not even a manifest.
    Unchanged {
        /// What is held -- unmoved by this pass.
        revisions: PublishedRevisions,
    },

    /// Nothing was offered because nothing was ready to publish.
    Waiting {
        /// Why.
        reason: WaitingReason,
    },

    /// The pass read something it will not act on without a human: a
    /// coherence problem in held platform state, a catalogue conflict, or
    /// the publication target's own validation refusing the whole
    /// snapshot.
    Refused {
        /// What was refused, in the refusing side's own words.
        reason: SafeDiagnostic,
    },

    /// The pass could not complete -- a transport failure reaching an
    /// input or the publication target, not a decision about what was
    /// read.
    Failed {
        /// What went wrong.
        detail: SafeDiagnostic,
    },
}

/// Why a pass found nothing ready to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WaitingReason {
    /// The derived runtime catalogue has no resources yet -- no
    /// application has published a release that declares one.
    ///
    /// ADR 0018 part 2: a catalogue can never legitimately be empty, so an
    /// empty one is not a refusal, it is the platform not being ready to
    /// publish yet. Composing and offering `tenants.json` or
    /// `data-sources.json` anyway, ahead of a catalogue, is exactly the
    /// partial state ADR 0023 part 4 (D3) supersedes ADR 0018's "create
    /// empty documents at startup" to avoid -- see that ADR's amendment.
    NoResources,
}

/// Whether a pass ran at all.
///
/// Mirrors [`crate::SweepResult`] for the same reason [`PassOutcome`]
/// mirrors `CheckOutcome`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PassResult {
    /// It ran, and this is what happened.
    Ran(PassOutcome),

    /// Another pass was still going, so this one did nothing.
    ///
    /// Skipped rather than queued, for the same reason a sweep is: a pass
    /// that overruns its interval means a read or the publication target
    /// is slow, and starting a second one behind it does not fix that.
    AlreadyRunning,
}
