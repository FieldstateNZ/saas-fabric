//! The background pass that publishes this environment's runtime state.
//!
//! [`start_sweeping`](super::start_sweeping)'s sibling, copied in shape
//! deliberately: the same "absent or zero starts nothing, the first tick is
//! immediate, a failed pass never stops the loop" rule applies for the same
//! reason -- a process that only wants to serve the API should be able to,
//! without a background task publishing behind it.

use std::sync::Arc;
use std::time::Duration;

use fabric_platform_management::{PassResult, PublicationState, RuntimePublisher};

use crate::config::PublicationConfig;

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
/// only whether a pass also runs unattended.
pub fn start_publishing(
    config: Option<&PublicationConfig>,
    publisher: Option<&Arc<RuntimePublisher>>,
    state: &Arc<PublicationState>,
) {
    let (Some(config), Some(publisher)) = (config, publisher) else {
        return;
    };

    if config.interval_seconds == 0 {
        tracing::info!("runtime publication is configured and its schedule is disabled");
        return;
    }

    let interval = Duration::from_secs(config.interval_seconds);
    let publisher = Arc::clone(publisher);
    let state = Arc::clone(state);

    tracing::info!(
        interval_seconds = config.interval_seconds,
        "this environment's runtime state will be published on a schedule"
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // The first tick fires immediately, which is what an operator
        // expects after a restart: the environment is published now, not
        // in a minute.
        ticker.tick().await;

        loop {
            publish_once(&publisher, &state).await;
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
async fn publish_once(publisher: &Arc<RuntimePublisher>, state: &Arc<PublicationState>) {
    if matches!(publisher.publish_once(state).await, PassResult::AlreadyRunning) {
        tracing::warn!("a publication pass overran its interval and was skipped");
    }
}

#[cfg(test)]
mod tests {
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

    /// Every method panics if called. None of them are, in the test below --
    /// `start_publishing` with a zero interval returns before a publisher is
    /// ever touched, which is the property under test.
    struct Untouched;

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
            unreachable!()
        }
        async fn publish(&self, _: &RuntimeSnapshot) -> Result<PublicationReport, PublicationError> {
            unreachable!()
        }
        fn describe(&self) -> String {
            unreachable!()
        }
    }

    fn untouched_publisher() -> Arc<RuntimePublisher> {
        Arc::new(RuntimePublisher::new(
            "lucentroot".to_owned(),
            Arc::new(Untouched),
            Arc::new(Untouched),
            Arc::new(Untouched),
            SystemClock::shared(),
        ))
    }

    #[tokio::test(start_paused = true)]
    async fn a_zero_interval_spawns_nothing() {
        let publisher = untouched_publisher();
        let state = Arc::new(PublicationState::new());

        start_publishing(
            Some(&PublicationConfig {
                namespace: "platform-system".to_owned(),
                interval_seconds: 0,
            }),
            Some(&publisher),
            &state,
        );

        // If a task had been spawned despite the zero interval, advancing
        // time would let it run and touch `Untouched`, panicking. Nothing
        // panicking, and nothing recorded, is the proof it never started.
        tokio::time::advance(std::time::Duration::from_secs(3600)).await;
        assert!(state.last_pass().is_none());
    }

    #[tokio::test]
    async fn no_publisher_configured_is_a_no_op() {
        // Absent or zero, `start_publishing` must not panic reaching for a
        // publisher it was not given -- the trigger route still works either
        // way, only the schedule is what this decides.
        start_publishing(
            Some(&PublicationConfig {
                namespace: "platform-system".to_owned(),
                interval_seconds: 60,
            }),
            None,
            &Arc::new(PublicationState::new()),
        );
    }
}
