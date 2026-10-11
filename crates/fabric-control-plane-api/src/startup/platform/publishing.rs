//! The background pass that publishes this environment's runtime state.
//!
//! [`start_sweeping`](super::start_sweeping)'s sibling, copied in shape
//! deliberately: the same "absent or zero starts nothing, the first tick is
//! immediate, a failed pass never stops the loop -- nor does a panicking
//! one" rule applies for the same reason -- a process that only wants to
//! serve the API should be able to, without a background task publishing
//! behind it. Each tick runs in a task of its own (`tick::survive_panic`),
//! so a panic in the publication target is logged, without its payload, and
//! the schedule continues.

use std::sync::Arc;
use std::time::Duration;

use fabric_platform_management::{PassResult, PublicationState, RuntimePublisher};
use tracing::Instrument;

use crate::config::PlatformManagementConfig;
use crate::startup::tick;

/// Starts the publication schedule, if this deployment has one to run.
///
/// # Why the host starts this and the rules crate does not
///
/// The same reason the sweep is the host's: `RuntimePublisher` takes a
/// snapshot and offers it, and is not a scheduler itself. The cadence is a
/// deployment's to choose, in `[platform_management.publication]`.
///
/// A zero interval, or no `publisher` at all (no platform managed, or no
/// publication target configured), starts nothing -- an operator's `POST
/// /api/platform/publication` still works either way; what this controls is
/// only whether a pass also runs unattended. Takes the whole
/// `PlatformManagementConfig`, not just its `publication` section, the same
/// as `start_sweeping` does: `environment` only labels the panic line's
/// span, and `RuntimePublisher` has no accessor for one to read instead.
pub fn start_publishing(
    config: Option<&PlatformManagementConfig>,
    publisher: Option<&Arc<RuntimePublisher>>,
    state: &Arc<PublicationState>,
) {
    let (Some(config), Some(publisher)) = (config, publisher) else {
        return;
    };
    let Some(publication) = config.publication.as_ref() else {
        return;
    };

    if publication.interval_seconds == 0 {
        tracing::info!("runtime publication is configured and its schedule is disabled");
        return;
    }

    let environment = config.environment.clone();
    let interval = Duration::from_secs(publication.interval_seconds);
    let publisher = Arc::clone(publisher);
    let state = Arc::clone(state);

    tracing::info!(
        interval_seconds = publication.interval_seconds,
        "this environment's runtime state will be published on a schedule"
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // The first tick fires immediately, which is what an operator
        // expects after a restart: the environment is published now, not
        // in a minute.
        ticker.tick().await;

        loop {
            let publisher = Arc::clone(&publisher);
            let state = Arc::clone(&state);
            // `survive_panic`'s panic event is emitted in this loop task,
            // not in the task it spawns for `work` -- so the span wraps
            // `survive_panic(...)` itself, below, as well as the block
            // passed to it.
            let span = tracing::info_span!("runtime_publication", environment = %environment);
            tick::survive_panic(
                "runtime publication",
                async move { publish_once(&publisher, &state).await }.instrument(span.clone()),
            )
            // Both halves: the outer covers the panic line, emitted in this
            // loop task; the inner covers `publish_once`'s own warn line,
            // which runs in the spawned task and would otherwise carry no
            // `environment`. Every test here would pass without either.
            .instrument(span)
            .await;
            ticker.tick().await;
        }
    });
}

/// One pass. `RuntimePublisher::publish_once` already logs its own
/// structured event for every outcome it returns (`event =
/// "control_plane.publication.pass"`), so only the one outcome it does
/// *not* log -- a pass skipped because another was still running -- is
/// logged here.
///
/// A failed pass is recorded and the loop continues, for the same reason a
/// sweep does not abandon its remaining environment: a target being briefly
/// unreachable would otherwise stop publication until somebody restarted the
/// process, and nothing would say so.
///
/// Nor does a panic end it, for the same reason and by the same mechanism as
/// `sweep_once`: `start_publishing` runs this call through
/// `tick::survive_panic`, inside a span carrying `environment` so the panic
/// line can be told apart from another environment's, and the schedule
/// proceeds regardless. `RuntimePublisher::environment` is `pub(super)`
/// inside `fabric-platform-management` with no accessor, which is why the
/// span is built from `start_publishing`'s own `environment` parameter
/// rather than read off the publisher.
async fn publish_once(publisher: &Arc<RuntimePublisher>, state: &Arc<PublicationState>) {
    if matches!(publisher.publish_once(state).await, PassResult::AlreadyRunning) {
        tracing::warn!("a publication pass overran its interval and was skipped");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use fabric_core::SystemClock;
    use fabric_platform_management::{
        CatalogueSourceError, ComponentDesired, DataSourceDeclaration, DataSourcesRead, DesiredRevision,
        EnvironmentWrite, Hold, PlacementRecord, PlacementsRead, PlatformRepository, Release,
        RuntimeCatalogueSource,
    };
    use fabric_runtime_publication::{
        CatalogDocument, PublicationError, PublicationReport, PublishedRevisions, RuntimeSnapshot,
    };

    use super::*;
    use crate::config::{PublicationConfig, RegistryBinding};

    /// A managed config with `publication` set to a given interval, the
    /// rest of it the shared shape `platform.rs`'s own `managed()` test
    /// helper uses.
    fn managed(interval_seconds: u64) -> PlatformManagementConfig {
        PlatformManagementConfig {
            environment: "lucentroot".to_owned(),
            registry: RegistryBinding::default(),
            observation: BTreeMap::new(),
            reconciliation_interval_seconds: 60,
            operation_timeout_seconds: 15,
            publication: Some(PublicationConfig {
                namespace: "platform-system".to_owned(),
                interval_seconds,
            }),
        }
    }

    /// Every method panics if called, except `RuntimePublication::current`,
    /// which counts every call -- including the one that panics -- and
    /// panics only on the first one.
    ///
    /// `current` records the call before it decides whether to panic:
    /// `fetch_add` always runs, and only the value it returns is asserted
    /// against. That ordering is what makes the counter trustworthy
    /// regardless of which test reads it. `a_zero_interval_spawns_nothing`
    /// needs "0 calls" to mean *no call reached this method at all* -- if
    /// the increment ran only after surviving the assert, a call that
    /// panicked would vanish from the count instead of falsifying "0",
    /// and a bug that spawned a task despite the zero interval could hide
    /// behind it. `a_panicking_pass_does_not_stop_the_schedule` needs the
    /// opposite read of the same count: the first, panicking call included,
    /// so a second call can be told from none.
    struct Untouched {
        current_calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl fabric_platform_management::DesiredState for Untouched {
        async fn components(
            &self,
            _: &str,
        ) -> Result<Vec<String>, fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
        async fn component(
            &self,
            _: &str,
            _: &str,
        ) -> Result<ComponentDesired, fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
        async fn advance(
            &self,
            _: &str,
            _: &str,
            _: &Release,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
        async fn roll_back(
            &self,
            _: &str,
            _: &str,
            _: &Release,
            _: &Hold,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
        async fn pause(
            &self,
            _: &str,
            _: &str,
            _: &Hold,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
        async fn resume(
            &self,
            _: &str,
            _: &str,
            _: &DesiredRevision,
            _: &str,
        ) -> Result<(), fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
    }

    #[async_trait::async_trait]
    impl fabric_platform_management::DataSourceState for Untouched {
        async fn read_data_sources(
            &self,
            _: &str,
        ) -> Result<DataSourcesRead, fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
        async fn write_data_sources(
            &self,
            _: &str,
            _: &[DataSourceDeclaration],
            _: Option<&DesiredRevision>,
            _: &str,
        ) -> Result<(), fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
    }

    #[async_trait::async_trait]
    impl fabric_platform_management::PlacementState for Untouched {
        async fn read_placements(
            &self,
            _: &str,
        ) -> Result<PlacementsRead, fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
        async fn write_placements(
            &self,
            _: &str,
            _: &[PlacementRecord],
            _: Option<&DesiredRevision>,
            _: &str,
        ) -> Result<(), fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
    }

    #[async_trait::async_trait]
    impl PlatformRepository for Untouched {
        async fn write_environment(
            &self,
            _: &str,
            _: EnvironmentWrite<'_>,
            _: &str,
        ) -> Result<(), fabric_platform_management::DesiredStateError> {
            unreachable!()
        }
    }

    #[async_trait::async_trait]
    impl RuntimeCatalogueSource for Untouched {
        async fn runtime_catalogue(&self) -> Result<CatalogDocument, CatalogueSourceError> {
            unreachable!()
        }
    }

    #[async_trait::async_trait]
    impl fabric_runtime_publication::RuntimePublication for Untouched {
        async fn current(&self) -> Result<PublishedRevisions, PublicationError> {
            let previous = self.current_calls.fetch_add(1, Ordering::SeqCst);
            assert!(previous != 0, "simulated target failure");
            Ok(PublishedRevisions::default())
        }
        async fn publish(&self, _: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
            unreachable!()
        }
        fn describe(&self) -> String {
            unreachable!()
        }
    }

    fn untouched_publisher(current_calls: &Arc<AtomicUsize>) -> Arc<RuntimePublisher> {
        Arc::new(RuntimePublisher::new(
            "lucentroot".to_owned(),
            Arc::new(Untouched {
                current_calls: Arc::clone(current_calls),
            }),
            Arc::new(Untouched {
                current_calls: Arc::clone(current_calls),
            }),
            Arc::new(Untouched {
                current_calls: Arc::clone(current_calls),
            }),
            SystemClock::shared(),
        ))
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_interval_spawns_nothing() {
        let current_calls = Arc::new(AtomicUsize::new(0));
        let publisher = untouched_publisher(&current_calls);
        let state = Arc::new(PublicationState::new());

        start_publishing(Some(&managed(0)), Some(&publisher), &state);

        // If a task had been spawned despite the zero interval, advancing
        // time would let it run a whole pass -- `current()` included, and
        // counted, rather than left to panic somewhere a `JoinHandle`
        // nobody awaits would swallow silently. The yield loop is what
        // makes that true: without it, `advance` alone never polls a
        // spawned loop task under paused time, so this assertion would
        // read "0 calls" whether or not one had been spawned -- see the
        // other two tests in this module for the same pattern.
        tokio::time::advance(std::time::Duration::from_secs(3600)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(current_calls.load(Ordering::SeqCst), 0);
        assert!(state.last_pass().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_platform_without_a_publication_section_spawns_nothing() {
        // The whole section is optional: a deployment that manages a platform
        // but publishes no runtime state omits it, and this must start
        // nothing rather than reach for an interval it was never given. The
        // guard used to live at the call site; now that it lives here, it
        // gets the same "0 calls" proof the zero-interval case has.
        let current_calls = Arc::new(AtomicUsize::new(0));
        let publisher = untouched_publisher(&current_calls);
        let state = Arc::new(PublicationState::new());
        let without_publication = PlatformManagementConfig {
            publication: None,
            ..managed(60)
        };

        start_publishing(Some(&without_publication), Some(&publisher), &state);

        tokio::time::advance(std::time::Duration::from_secs(3600)).await;
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(current_calls.load(Ordering::SeqCst), 0);
        assert!(state.last_pass().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_panicking_pass_does_not_stop_the_schedule() {
        // `Untouched::current` panics on its first call and counts every
        // call thereafter -- see its own rustdoc. The spawned tick task's
        // panic prints a backtrace to stderr even though this test passes;
        // that is tokio reporting the panic the way it always does for a
        // task whose `JoinHandle` is awaited, and is expected here, not a
        // sign something needs fixing.
        let current_calls = Arc::new(AtomicUsize::new(0));
        let publisher = untouched_publisher(&current_calls);
        let state = Arc::new(PublicationState::new());

        start_publishing(Some(&managed(1)), Some(&publisher), &state);

        // The immediate first tick panics; advancing past two more
        // one-second intervals must still reach `current()` again. Yielding
        // between advances gives the spawned loop task, and the per-tick
        // task `survive_panic` spawns inside it, a chance to actually run
        // under paused time.
        for _ in 0..3 {
            tokio::time::advance(std::time::Duration::from_secs(1)).await;
            for _ in 0..10 {
                tokio::task::yield_now().await;
            }
        }

        let seen = current_calls.load(Ordering::SeqCst);
        assert!(
            seen >= 2,
            "a loop that died on the first panic would leave this at 1; saw {seen}"
        );
    }

    #[tokio::test]
    async fn no_publisher_configured_is_a_no_op() {
        // Absent or zero, `start_publishing` must not panic reaching for a
        // publisher it was not given -- the trigger route still works either
        // way, only the schedule is what this decides.
        start_publishing(Some(&managed(60)), None, &Arc::new(PublicationState::new()));
    }
}
