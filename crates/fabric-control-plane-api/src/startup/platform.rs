//! Composing Platform Management, and starting what advances an environment.

use std::sync::Arc;

use fabric_control_plane::PlatformBinding;
use fabric_core::Clock;
use fabric_platform_management::{
    ChartIndex, DesiredState, PlatformDesiredState, PlatformManagement, Registry,
};
use fabric_registry::{HelmCharts, OciRegistry};

mod sweeping;

pub(super) use sweeping::start_sweeping;

use crate::config::PlatformManagementConfig;

/// Builds Platform Management, if this deployment does platform management.
///
/// # What is configuration, and what is not any more
///
/// The environment, the registry and the cadence are a deployment's: they are
/// facts about where this control plane runs. The *repository* and its
/// credential are not — an operator installs the Platform Management GitHub
/// App and picks a repository, and the platform stores what it learns doing
/// so.
///
/// So there is nothing to build a repository from at startup, and the binding
/// starts unconnected. A control plane that refused to start without one could
/// not be used to connect one.
///
/// # `None` still means unconfigured, and still never means "could not build"
///
/// A deployment that states this section and gets *its own* configuration
/// wrong fails to start. That rule has not moved; what moved is which things
/// are configuration. An absent integration is now legitimate runtime state
/// rather than a misconfiguration, and it is reported rather than fatal.
///
/// # The operation budget, plus one call, has to fit inside a request
///
/// The platform binding holds a lock across every desired-state call, so an
/// operator's disconnect waits for whatever is already running. If one
/// operation could outlast one request, the disconnect would be cut off by the
/// API's request timeout before it took effect — the operator told `504`, the
/// platform still pointed at the repository they asked it to forget. So this
/// refuses to start rather than leaving that one slow Git host away.
///
/// The budget alone is not the bound. It stops the adapter *starting* a call it
/// cannot afford and never abandons one already sent — a write abandoned
/// mid-flight would release the binding while it might still land — so an
/// operation runs for the budget plus one `git_host.http_timeout_seconds`. That
/// sum is what must fit, and the refusal names all three values.
///
/// What it bounds is the *unbind*: the rest of a disconnect runs after it, so a
/// sum only just inside the request leaves that tail no room. The check is a
/// floor rather than a recommendation; the defaults leave five seconds.
///
/// # Errors
///
/// Returns a message naming the field. Never a credential.
pub fn establish(
    config: Option<&PlatformManagementConfig>,
    http_timeout_seconds: u64,
    request_timeout_seconds: u64,
    clock: &Arc<dyn Clock>,
) -> Result<Option<PlatformBinding>, String> {
    let Some(config) = config else {
        return Ok(None);
    };

    if config.operation_timeout_seconds == 0 {
        return Err(
            "platform_management.operation_timeout_seconds must be at least 1: zero would time \
             out every operation immediately"
                .to_owned(),
        );
    }

    // Saturating, so a deployment that wrote something absurd is refused rather
    // than wrapping round into a sum that looks small enough.
    let longest = config
        .operation_timeout_seconds
        .saturating_add(http_timeout_seconds);

    if longest >= request_timeout_seconds {
        return Err(format!(
            "platform_management.operation_timeout_seconds ({}) plus git_host.http_timeout_seconds \
             ({http_timeout_seconds}) must be less than request_timeout_seconds \
             ({request_timeout_seconds}): an operator's disconnect waits for the operation already \
             in flight, which runs for the budget plus the one call the budget cannot cut short, \
             and all of that must finish inside one request",
            config.operation_timeout_seconds
        ));
    }

    let registry = OciRegistry::new(
        &config.registry.base_url,
        &config.registry.host,
        config.registry.http_timeout_seconds,
    )?;

    // Anonymous, like the image registry, and for the same reason: a chart
    // repository serves its index to anybody, so there is no credential here
    // to be conflated with the platform application's authority.
    let charts = HelmCharts::new(config.registry.http_timeout_seconds)?;

    let repository = PlatformDesiredState::unconnected();

    Ok(Some(PlatformBinding {
        service: Arc::new(PlatformManagement::new(
            Arc::new(registry) as Arc<dyn Registry>,
            Arc::new(charts) as Arc<dyn ChartIndex>,
            Arc::clone(&repository) as Arc<dyn DesiredState>,
            Arc::clone(clock),
        )),
        repository,
        environment: config.environment.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RegistryBinding;

    fn managed(operation_timeout_seconds: u64) -> PlatformManagementConfig {
        PlatformManagementConfig {
            environment: "lucentroot".to_owned(),
            registry: RegistryBinding::default(),
            reconciliation_interval_seconds: 60,
            operation_timeout_seconds,
        }
    }

    fn clock() -> Arc<dyn Clock> {
        fabric_core::SystemClock::shared()
    }

    #[test]
    fn a_budget_that_could_outlast_a_request_is_refused_at_startup() {
        // The failure this prevents is silent: the operator's disconnect would
        // be cut off by the request timeout while still waiting for the
        // binding, so nothing would be released and they would be told 504.
        for operation in [30, 45] {
            // `err()` rather than `expect_err`: the success type is not
            // `Debug`, and making a composition-root binding printable to
            // please a test would be the wrong way round.
            let message = establish(Some(&managed(operation)), 10, 30, &clock())
                .err()
                .expect("a budget at or over the request timeout must not start");

            assert!(message.contains("operation_timeout_seconds"), "{message}");
            assert!(message.contains("request_timeout_seconds"), "{message}");
        }
    }

    #[test]
    fn a_budget_that_fits_alone_but_not_with_one_call_is_refused_at_startup() {
        // The whole point of the sum. Twenty-five is comfortably inside a
        // thirty-second request on its own, and an operation that spends it and
        // then waits out one ten-second call to the host is not — so the
        // disconnect queued behind it is cut off at thirty having released
        // nothing, which is exactly the silent failure the check exists for.
        let message = establish(Some(&managed(25)), 10, 30, &clock())
            .err()
            .expect("a budget that only fits without the call it cannot cut short must not start");

        assert!(message.contains("operation_timeout_seconds"), "{message}");
        assert!(message.contains("http_timeout_seconds"), "{message}");
        assert!(message.contains("request_timeout_seconds"), "{message}");
    }

    #[test]
    fn a_zero_budget_is_refused_at_startup() {
        let message = establish(Some(&managed(0)), 10, 30, &clock())
            .err()
            .expect("zero would time every operation out immediately");

        assert!(message.contains("operation_timeout_seconds"), "{message}");
    }

    #[test]
    fn a_budget_inside_the_request_starts() {
        // The shipped defaults: fifteen, plus the ten a call may take, inside
        // thirty with five to spare for the rest of a disconnect.
        assert!(establish(Some(&managed(15)), 10, 30, &clock()).is_ok());
    }

    #[test]
    fn a_deployment_that_manages_no_platform_is_not_asked_about_budgets() {
        // Absent is deliberately unconfigured, not misconfigured. Validating a
        // section nobody wrote would turn "we do no platform management" into a
        // startup failure.
        assert!(establish(None, 10, 1, &clock())
            .expect("absent is fine")
            .is_none());
    }
}
