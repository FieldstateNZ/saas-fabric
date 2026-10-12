//! What [`FakePublication`](super::FakePublication) answers when inspected:
//! complete at its last accepted offer's stamp, incomplete once an offer
//! was interrupted after writing something, until an offer is accepted.

use fabric_runtime_publication::{
    CompletenessGap, DocumentKind, DocumentOutcome, DocumentRevision, GapKind, PublicationCompleteness,
    PublicationError, RecordStamp, RecordedPart,
};

use super::held::State;
use super::FakePublication;

/// The fake's record, and whether an interrupted offer left it behind.
#[derive(Default)]
pub(super) struct Inspection {
    pub(super) record: Option<RecordStamp>,
    pub(super) interrupted: bool,
    /// Answered in place of the modelled state, once, when set.
    pub(super) scripted: Option<PublicationCompleteness>,
    /// Every call to `inspect`.
    pub(super) calls: usize,
}

impl FakePublication {
    /// Answers `completeness` the next time the target is inspected.
    pub(crate) fn script_inspection(&self, completeness: PublicationCompleteness) {
        self.lock().inspection.scripted = Some(completeness);
    }

    /// How many times the target was inspected.
    pub(crate) fn inspections(&self) -> usize {
        self.lock().inspection.calls
    }
}

impl State {
    pub(super) fn inspect(&mut self) -> PublicationCompleteness {
        self.inspection.calls += 1;
        if let Some(scripted) = self.inspection.scripted.take() {
            return scripted;
        }
        match (self.inspection.record, self.inspection.interrupted) {
            (None, _) => PublicationCompleteness::Unpublished,
            (Some(record), false) => PublicationCompleteness::Complete { record },
            (Some(record), true) => PublicationCompleteness::Incomplete {
                record,
                gaps: vec![CompletenessGap {
                    part: RecordedPart::DataSources,
                    kind: GapKind::RevisionMismatch,
                }],
            },
        }
    }
}

impl State {
    /// Settles the three documents in write order, stopping at the first
    /// interrupted one. Accepted, the offer becomes the held record;
    /// interrupted after a write, it leaves the target incomplete.
    pub(super) fn settle_offer(
        &mut self,
        offered: [(DocumentKind, DocumentRevision); 3],
    ) -> Result<[DocumentOutcome; 3], PublicationError> {
        let written = self.writes.len();
        let mut outcomes = [DocumentOutcome::Unchanged; 3];
        for (outcome, (document, revision)) in outcomes.iter_mut().zip(offered) {
            match self.settle_unless_interrupted(document, revision) {
                Ok(settled) => *outcome = settled,
                Err(error) => {
                    self.inspection.interrupted |= self.writes.len() > written;
                    return Err(error);
                }
            }
        }
        self.inspection.interrupted = false;
        self.inspection.record = Some(fake_stamp(self.publish_attempts));
        Ok(outcomes)
    }
}

/// A record stamp that differs on every attempt: this fake keeps no record.
pub(super) fn fake_stamp(attempt: usize) -> RecordStamp {
    let generation = u64::try_from(attempt).unwrap_or(u64::MAX);
    RecordStamp {
        generation,
        sha256: fabric_core::RecordDigest::from_bytes([u8::try_from(attempt % 256).unwrap_or(0); 32]),
    }
}
