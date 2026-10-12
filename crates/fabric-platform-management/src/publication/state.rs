//! Whether a publication pass is running now, and what the last one did.
//!
//! In the 121-150 band: one state type, whose four records (last pass,
//! published tenants, readiness evidence, completeness) are written
//! together by `record`. The record types are in `pass_records.rs`.

use std::sync::Mutex;

use crate::publication::completeness_observation::CompletenessObservation;
use crate::publication::outcome::PassOutcome;
use crate::publication::pass_records::{LastPass, PublishedTenants};
use crate::publication::published::Accepted;
use crate::publication::readiness::ReadinessReport;
use crate::publication::readiness_evidence::ReadinessEvidence;
use crate::running_guard::RunningFlag;

/// Guards against two publication passes at once, and remembers the last
/// one.
///
/// [`crate::SweepState`]'s sibling, for the same reason the guard and the
/// record live together there: "is a pass running" and "what did the last
/// one do" are one console question, not two, and a caller asking only one
/// of them would be asking half of it.
#[derive(Debug, Default)]
pub struct PublicationState {
    /// Whether a pass is in progress.
    ///
    /// Guards re-entry rather than a real race: two passes running at once
    /// would both decide what to publish against the same held revisions,
    /// and the second's write is refused as stale by the publication target
    /// itself (ADR 0018 part 6). This exists to save the wasted read and
    /// write, not to close a race the port does not already close.
    ///
    /// A `RunningFlag`, not a bare `AtomicBool`, for the same reason
    /// `SweepState::running` is one: the swap that claims it and the guard
    /// that releases it are paired inside `RunningFlag::try_enter`, so
    /// neither can happen without the other.
    pub(super) running: RunningFlag,

    /// What the last completed pass found.
    last: Mutex<Option<LastPass>>,

    /// The tenants the last pass that published or found nothing to change
    /// left in the held tenants document. Kept across a later pass that
    /// failed or was refused, because such a pass wrote nothing.
    published: Mutex<Option<PublishedTenants>>,

    /// What the readiness report reads, from the same pass as `published`.
    evidence: Mutex<Option<ReadinessEvidence>>,

    /// What the last pass's inspection of the target found, whatever the
    /// pass's outcome. `None` until a pass has inspected it.
    completeness: Mutex<Option<CompletenessObservation>>,
}

impl PublicationState {
    /// A publication state with nothing recorded yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// What the last completed pass found, if one has run.
    ///
    /// `None` means nothing has run yet, which is its own answer -- the
    /// same three-way distinction [`crate::SweepState::last_check`] draws
    /// between "never checked", "checked, nothing to do" and "checked,
    /// failed".
    #[must_use]
    pub fn last_pass(&self) -> Option<LastPass> {
        self.last.lock().ok().and_then(|last| last.clone())
    }

    /// The tenants the held tenants document carries, as of the last pass
    /// that published or found nothing to change. `None` until one has, in
    /// this process: the console then cannot show a tenant as published.
    #[must_use]
    pub fn published_tenants(&self) -> Option<PublishedTenants> {
        self.published.lock().ok().and_then(|published| published.clone())
    }

    /// What the last pass's inspection of the target found. `None` until a
    /// pass has inspected it, in this process.
    #[must_use]
    pub fn completeness(&self) -> Option<CompletenessObservation> {
        self.completeness
            .lock()
            .ok()
            .and_then(|observed| observed.clone())
    }

    /// The readiness gates (D01-18), as of the last pass that published or
    /// found nothing to change, with the documents gate also held to the
    /// last inspection. All three are unmet until a pass has been accepted.
    #[must_use]
    pub fn readiness(&self) -> ReadinessReport {
        let evidence = self.evidence.lock().ok().and_then(|evidence| evidence.clone());
        ReadinessReport::of(evidence.as_ref(), self.completeness().as_ref())
    }

    /// The publication record the last pass that published or found nothing
    /// to change left held, which a replica must report to have applied it
    /// (G7). `None` until one has, in this process.
    #[must_use]
    pub fn expected_record(&self) -> Option<fabric_runtime_publication::RecordStamp> {
        let evidence = self.evidence.lock().ok()?;
        evidence.as_ref().map(|evidence| evidence.record)
    }

    /// Records how a pass ended, what it left published if it did, and what
    /// its inspection of the target found.
    pub(super) fn record(
        &self,
        at_unix_seconds: u64,
        outcome: PassOutcome,
        accepted: Option<Accepted>,
        completeness: fabric_runtime_publication::PublicationCompleteness,
    ) {
        if let Some(accepted) = accepted {
            if let Ok(mut held) = self.published.lock() {
                *held = Some(PublishedTenants {
                    at_unix_seconds,
                    tenants: accepted.tenants,
                });
            }
            if let Ok(mut evidence) = self.evidence.lock() {
                *evidence = Some(accepted.evidence);
            }
        }
        if let Ok(mut observed) = self.completeness.lock() {
            *observed = Some(CompletenessObservation {
                at_unix_seconds,
                completeness,
            });
        }
        if let Ok(mut last) = self.last.lock() {
            *last = Some(LastPass {
                at_unix_seconds,
                outcome,
            });
        }
    }
}
