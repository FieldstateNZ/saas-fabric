//! The documents gate: every document published by an accepted pass, and
//! the target last observed holding that same generation whole (G3).

use fabric_runtime_publication::{PublicationCompleteness, PublishedRevisions, RecordStamp};

use super::completeness_observation::CompletenessObservation;
use super::readiness::GateStatus;

/// Met only with the accepted revisions naming all three documents and
/// `observed` complete at exactly the accepted record, generation and
/// digest. A target complete at any other record is held: the accepted
/// identity, connector and reader evidence describe the accepted record,
/// not that one. Without an accepted pass, says only that, beside what
/// completeness adds.
pub(super) fn documents(
    accepted: Option<(&PublishedRevisions, RecordStamp)>,
    observed: Option<&CompletenessObservation>,
) -> GateStatus {
    let mut reasons: Vec<String> = match accepted {
        None => vec!["no publication pass has been accepted since the control plane started".to_owned()],
        Some((revisions, _)) => [
            ("tenants", revisions.tenants),
            ("data-sources", revisions.data_sources),
            ("catalog", revisions.catalog),
        ]
        .into_iter()
        .filter(|(_, revision)| revision.is_none())
        .map(|(document, _)| format!("{document} is not published"))
        .collect(),
    };
    reasons.extend(incompleteness(observed));
    reasons.extend(record_mismatch(accepted.map(|(_, record)| record), observed));
    if reasons.is_empty() {
        GateStatus::Met
    } else {
        GateStatus::Unmet { reasons }
    }
}

/// Why the last observation does not show a whole generation, in an
/// operator's words; `None` when it does.
#[must_use]
pub fn incompleteness(observed: Option<&CompletenessObservation>) -> Option<String> {
    let Some(observed) = observed else {
        return Some(
            "no publication pass has inspected the target since the control plane started".to_owned(),
        );
    };
    match &observed.completeness {
        PublicationCompleteness::Complete { .. } => None,
        PublicationCompleteness::Incomplete { record, gaps } => Some(format!(
            "the target does not hold generation {} whole: {}",
            record.generation,
            gaps.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; ")
        )),
        PublicationCompleteness::Unpublished => Some("the target holds no publication record".to_owned()),
        PublicationCompleteness::Unobserved { reason } => {
            Some(format!("the target's completeness is unobserved: {reason}"))
        }
    }
}

/// Why a target observed complete does not hold the accepted record; `None`
/// when it does, or when it was not observed complete.
fn record_mismatch(
    accepted: Option<RecordStamp>,
    observed: Option<&CompletenessObservation>,
) -> Option<String> {
    let accepted = accepted?;
    let PublicationCompleteness::Complete { record } = observed?.completeness else {
        return None;
    };
    (record != accepted).then(|| {
        format!(
            "the target holds generation {} ({}) whole, not generation {} ({}) that the last accepted pass published",
            record.generation,
            short(record),
            accepted.generation,
            short(accepted)
        )
    })
}

fn short(record: RecordStamp) -> String {
    record.sha256.to_string().chars().take(12).collect()
}
