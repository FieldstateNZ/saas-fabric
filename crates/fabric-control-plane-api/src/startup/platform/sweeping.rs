//! The background pass that advances an environment.

use std::sync::Arc;
use std::time::Duration;

use fabric_control_plane::PlatformBinding;
use fabric_platform_management::{PlatformManagement, SweepResult, SweepState};
use tracing::Instrument;

use crate::config::PlatformManagementConfig;
use crate::startup::tick;

/// Starts the sweep, if this deployment has one to run.
///
/// # Why the host starts this and the crate does not
///
/// The same reason the client reconciliation loop is the host's: a process
/// that only wants to serve the API — a test, a replica running read-only —
/// should be able to have one without a background task advancing
/// environments behind it. The rules crate takes an environment and a state
/// and is not a scheduler; the cadence is a deployment's to choose.
///
/// A zero interval, or no configuration at all, starts nothing. That is how a
/// deployment observes an environment without advancing it.
pub fn start_sweeping(
    config: Option<&PlatformManagementConfig>,
    platform: Option<&PlatformBinding>,
    sweeps: &Arc<SweepState>,
) {
    let (Some(config), Some(platform)) = (config, platform) else {
        return;
    };

    if config.reconciliation_interval_seconds == 0 {
        tracing::info!(
            environment = config.environment,
            "platform management is configured and its sweep is disabled"
        );
        return;
    }

    let environment = platform.environment.clone();
    let interval = Duration::from_secs(config.reconciliation_interval_seconds);
    let platform = Arc::clone(&platform.service);
    let sweeps = Arc::clone(sweeps);

    tracing::info!(
        environment,
        interval_seconds = config.reconciliation_interval_seconds,
        "platform management will advance this environment"
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // The first tick fires immediately, which is what an operator expects
        // after a restart: the environment is checked now, not in a minute.
        ticker.tick().await;

        loop {
            let platform = Arc::clone(&platform);
            let sweeps = Arc::clone(&sweeps);
            // `survive_panic`'s panic event is emitted in this loop task,
            // not in the task it spawns for `work` -- so the span has to
            // wrap `survive_panic(...)` itself, below, not the `async move`
            // block passed to it.
            let span = tracing::info_span!("platform_sweep", environment = %environment);
            let tick_environment = environment.clone();
            tick::survive_panic("platform sweep", async move {
                sweep_once(&platform, &tick_environment, &sweeps).await;
            })
            // Must wrap `survive_panic(...)`, not the `async move` block
            // above -- moving `.instrument(span)` inside would attach the
            // span to the spawned task instead, leaving the panic line
            // without an environment, and every test here would still pass.
            .instrument(span)
            .await;
            ticker.tick().await;
        }
    });
}

/// One pass, with its outcome logged.
///
/// A failure here is recorded and the loop continues. The reason it must not
/// end is the same reason a sweep does not abandon its remaining components: a
/// registry being briefly unreachable would otherwise stop an environment
/// advancing until somebody restarted the process, and nothing would say so.
///
/// Nor does a panic end it. `start_sweeping` runs this call through
/// `tick::survive_panic`, in a task of its own, so a panic unwinding out of
/// an adapter is caught there as a join failure rather than taking the loop
/// task down with it; the panic is logged as a fixed sentence, never its
/// payload, inside a span carrying `environment` so it can be told apart
/// from another environment's, and the next tick proceeds regardless.
async fn sweep_once(platform: &Arc<PlatformManagement>, environment: &str, sweeps: &Arc<SweepState>) {
    match platform.sweep(environment, sweeps).await {
        Ok(SweepResult::NotConnected) => {
            // Every tick until an operator connects one. Not logged at all:
            // a minute's interval would fill a log with the fact that nobody
            // has done something yet, and the console already says so.
        }
        Ok(SweepResult::AlreadyRunning) => {
            tracing::warn!(
                environment,
                "a platform sweep overran its interval and was skipped"
            );
        }
        Ok(SweepResult::Ran(sweep)) => {
            for (component, swept) in &sweep.components {
                if let fabric_platform_management::Swept::Advanced { from, to } = swept {
                    tracing::info!(environment, component, %from, %to, "advanced");
                }
            }
        }
        Err(error) => {
            tracing::warn!(environment, error = %error, "a platform sweep could not run");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use fabric_core::Clock;
    use fabric_platform_management::{
        ChartIndex, ComponentDesired, DataSourceState, DataSources, DesiredRevision, DesiredState,
        DesiredStateError, Hold, Placements, PlatformDesiredState, PlatformRepository, PublicationState,
        Registry, RegistryError, Release, Resolved, Version,
    };

    use super::*;
    use crate::config::RegistryBinding;

    /// A desired state whose `components` panics on the first call and counts
    /// every call, proving a panicking sweep tick does not end the schedule.
    /// Every other method is `unreachable!()`: `components` is the first
    /// thing a sweep reads (`PlatformManagement::sweep_once`), so a panic
    /// there is reached before anything else in this fake could be.
    ///
    /// `fetch_add` runs, and is recorded, before the `assert!` decides
    /// whether to panic -- the call that panics is still counted, which is
    /// what lets `a_panicking_sweep_does_not_stop_the_schedule` tell "died
    /// after the first call" from "never called again" by the count alone.
    struct PanicsOnceThenCounts {
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl DesiredState for PanicsOnceThenCounts {
        async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
            let previous = self.calls.fetch_add(1, Ordering::SeqCst);
            assert!(previous != 0, "simulated adapter failure");
            Ok(Vec::new())
        }

        async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
            unreachable!("components() runs first")
        }

        async fn advance(
            &self,
            _: &str,
            _: &str,
            _: &Release,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            unreachable!("components() runs first")
        }

        async fn roll_back(
            &self,
            _: &str,
            _: &str,
            _: &Release,
            _: &Hold,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            unreachable!("components() runs first")
        }

        async fn pause(
            &self,
            _: &str,
            _: &str,
            _: &Hold,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            unreachable!("components() runs first")
        }

        async fn resume(
            &self,
            _: &str,
            _: &str,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), DesiredStateError> {
            unreachable!("components() runs first")
        }
    }

    /// Never called: `PanicsOnceThenCounts::components` panics before a
    /// sweep can reach either port.
    struct NeverAsked;

    #[async_trait::async_trait]
    impl Registry for NeverAsked {
        async fn tags(&self, _: &str) -> Result<Vec<String>, RegistryError> {
            unreachable!("a sweep never reaches the registry in this test")
        }

        async fn resolve(&self, _: &str, _: &str) -> Result<Option<Resolved>, RegistryError> {
            unreachable!("a sweep never reaches the registry in this test")
        }
    }

    #[async_trait::async_trait]
    impl ChartIndex for NeverAsked {
        async fn versions(&self, _: &str, _: &str) -> Result<Vec<Version>, RegistryError> {
            unreachable!("a sweep never reaches the chart index in this test")
        }
    }

    fn managed() -> PlatformManagementConfig {
        PlatformManagementConfig {
            environment: "lucentroot".to_owned(),
            registry: RegistryBinding::default(),
            observation: BTreeMap::new(),
            reconciliation_interval_seconds: 1,
            operation_timeout_seconds: 15,
            publication: None,
        }
    }

    /// Built the way `establish` builds it: `service` is over the fake under
    /// test, and `repository`/`data_sources`/`placements` are the real,
    /// unconnected type -- the same shape a fresh deployment starts with.
    /// `start_sweeping` never touches the latter three; they exist only
    /// because `PlatformBinding` requires them.
    fn binding_over(desired_state: PanicsOnceThenCounts, clock: &Arc<dyn Clock>) -> PlatformBinding {
        let repository = PlatformDesiredState::unconnected();
        let data_sources = Arc::new(DataSources::new(
            Arc::clone(&repository) as Arc<dyn DataSourceState>
        ));
        let placements = Arc::new(Placements::new(
            Arc::clone(&repository) as Arc<dyn PlatformRepository>,
            Arc::clone(clock),
        ));

        PlatformBinding {
            service: Arc::new(PlatformManagement::new(
                Arc::new(NeverAsked) as Arc<dyn Registry>,
                Arc::new(NeverAsked) as Arc<dyn ChartIndex>,
                Arc::new(desired_state) as Arc<dyn DesiredState>,
                Arc::clone(clock),
            )),
            repository,
            data_sources,
            placements,
            environment: "lucentroot".to_owned(),
            publisher: None,
            publication: Arc::new(PublicationState::new()),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_sweep_does_not_stop_the_schedule() {
        let calls = Arc::new(AtomicUsize::new(0));
        let clock = fabric_core::SystemClock::shared();
        let platform = binding_over(
            PanicsOnceThenCounts {
                calls: Arc::clone(&calls),
            },
            &clock,
        );
        let sweeps = Arc::new(SweepState::default());

        start_sweeping(Some(&managed()), Some(&platform), &sweeps);

        // The immediate first tick panics; advancing past two more one-second
        // intervals must still reach `components()` again. Yielding between
        // advances gives the spawned loop task, and the per-tick task
        // `survive_panic` spawns inside it, a chance to actually run under
        // paused time -- nothing here is driven by a real clock.
        for _ in 0..3 {
            tokio::time::advance(std::time::Duration::from_secs(1)).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
        }

        let seen = calls.load(Ordering::SeqCst);
        assert!(
            seen >= 2,
            "a loop that died on the first panic would leave this at 1; saw {seen}"
        );
    }
}
