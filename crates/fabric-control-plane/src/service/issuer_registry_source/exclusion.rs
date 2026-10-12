//! A routing refusal as the exclusion kind the publication reports.

use fabric_client_model::lifecycle::ReadinessCondition;
use fabric_client_model::{ClientLifecycle, LifecycleRefusal};
use fabric_platform_management::Exclusion;

/// The routing refusal as an exclusion kind, so evidence still pending is
/// never reported as a recorded retirement. A not-ready refusal names each
/// unmet condition with the evidence the lifecycle holds for it.
pub(super) fn exclusion(refusal: &LifecycleRefusal, lifecycle: &ClientLifecycle) -> Exclusion {
    match refusal {
        LifecycleRefusal::Retired { state, .. } => Exclusion::Retired {
            state: state.to_string(),
        },
        _ => Exclusion::NotReady {
            unmet: match lifecycle {
                ClientLifecycle::Incomplete { unmet } => unmet
                    .iter()
                    .map(|unmet| format!("{}: {}", condition(unmet.condition), unmet.detail))
                    .collect(),
                _ => vec![refusal.to_string()],
            },
        },
    }
}

const fn condition(condition: ReadinessCondition) -> &'static str {
    match condition {
        ReadinessCondition::Provisioning => "provisioning",
        ReadinessCondition::Identity => "identity",
        ReadinessCondition::Publication => "publication",
    }
}
