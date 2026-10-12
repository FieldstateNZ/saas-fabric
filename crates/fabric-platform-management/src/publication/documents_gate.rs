//! The documents gate: every document published by an accepted pass, and
//! the target last observed holding that generation whole (G3).

use fabric_runtime_publication::{PublicationCompleteness, PublishedRevisions};

use super::completeness_observation::CompletenessObservation;
use super::readiness::GateStatus;

/// Met only with `revisions` naming all three documents and `observed`
/// complete. Without accepted revisions, says only that no pass was
/// accepted, beside what completeness adds.
pub(super) fn documents(
    revisions: Option<&PublishedRevisions>,
    observed: Option<&CompletenessObservation>,
) -> GateStatus {
    let mut reasons: Vec<String> = match revisions {
        None => vec!["no publication pass has been accepted since the control plane started".to_owned()],
        Some(revisions) => [
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
