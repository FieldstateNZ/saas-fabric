//! `write_environment` tags and untags *both* revisions against the one
//! generation they were read under -- the twin of what
//! `binding/data_sources_tests.rs` and `binding/placements_tests.rs` each
//! prove for their own single document, applied to the pair at once.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{FieldName, IsolationModelDocument};
use tokio::sync::Notify;

use super::PlatformDesiredState;
use crate::{
    ComponentDesired, DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision, DesiredState,
    DesiredStateError, EnvironmentWrite, PlacementRecord, PlacementState, PlacementsRead, PlatformRepository,
};

fn placement(tenant: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).expect("a valid tenant id"),
        logical: LogicalDataSourceName::try_new("primary").expect("a valid logical data source name"),
        data_source: DataSourceId::try_new("shared-a").expect("a valid data source id"),
        isolation: IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").expect("a valid field name"),
            value: tenant.to_owned(),
        },
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

/// A repository that answers every port and records every
/// `write_environment` call as the pair of `at`s it saw.
struct Repo {
    declarations: Mutex<Vec<DataSourceDeclaration>>,
    data_sources_revision: Mutex<Option<u64>>,
    placements: Mutex<Vec<PlacementRecord>>,
    placements_revision: Mutex<Option<u64>>,
    seen: Mutex<Vec<(Option<String>, Option<String>)>>,
    gate: Option<Arc<Notify>>,
    started: Arc<Notify>,
}

impl Repo {
    fn new(data_sources_revision: Option<u64>, placements_revision: Option<u64>) -> Arc<Self> {
        Arc::new(Self {
            declarations: Mutex::new(Vec::new()),
            data_sources_revision: Mutex::new(data_sources_revision),
            placements: Mutex::new(Vec::new()),
            placements_revision: Mutex::new(placements_revision),
            seen: Mutex::new(Vec::new()),
            gate: None,
            started: Arc::new(Notify::new()),
        })
    }

    fn gated(
        data_sources_revision: Option<u64>,
        placements_revision: Option<u64>,
    ) -> (Arc<Self>, Arc<Notify>) {
        let gate = Arc::new(Notify::new());
        (
            Arc::new(Self {
                declarations: Mutex::new(Vec::new()),
                data_sources_revision: Mutex::new(data_sources_revision),
                placements: Mutex::new(Vec::new()),
                placements_revision: Mutex::new(placements_revision),
                seen: Mutex::new(Vec::new()),
                gate: Some(Arc::clone(&gate)),
                started: Arc::new(Notify::new()),
            }),
            gate,
        )
    }

    fn seen(&self) -> Vec<(Option<String>, Option<String>)> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

#[async_trait::async_trait]
impl DesiredState for Repo {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(Vec::new())
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        Err(DesiredStateError::NotFound {
            what: "unused".to_owned(),
        })
    }

    async fn advance(
        &self,
        _: &str,
        _: &str,
        _: &crate::Release,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn roll_back(
        &self,
        _: &str,
        _: &str,
        _: &crate::Release,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn pause(
        &self,
        _: &str,
        _: &str,
        _: &crate::Hold,
        _: &DesiredRevision,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }

    async fn resume(&self, _: &str, _: &str, _: &DesiredRevision, _: &str) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl DataSourceState for Repo {
    async fn read_data_sources(&self, _: &str) -> Result<DataSourcesRead, DesiredStateError> {
        Ok(DataSourcesRead {
            revision: (*self
                .data_sources_revision
                .lock()
                .unwrap_or_else(PoisonError::into_inner))
            .map(|revision| DesiredRevision::new(revision.to_string())),
            declarations: self
                .declarations
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        })
    }

    async fn write_data_sources(
        &self,
        _: &str,
        _: &[DataSourceDeclaration],
        _: Option<&DesiredRevision>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl PlacementState for Repo {
    async fn read_placements(&self, _: &str) -> Result<PlacementsRead, DesiredStateError> {
        Ok(PlacementsRead {
            revision: (*self
                .placements_revision
                .lock()
                .unwrap_or_else(PoisonError::into_inner))
            .map(|revision| DesiredRevision::new(revision.to_string())),
            placements: self
                .placements
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        })
    }

    async fn write_placements(
        &self,
        _: &str,
        _: &[PlacementRecord],
        _: Option<&DesiredRevision>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl PlatformRepository for Repo {
    async fn write_environment(
        &self,
        _: &str,
        write: EnvironmentWrite<'_>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).push((
            write.data_sources.1.map(|revision| revision.as_str().to_owned()),
            write.placements.1.map(|revision| revision.as_str().to_owned()),
        ));

        self.started.notify_one();

        if let Some(gate) = &self.gate {
            gate.notified().await;
        }

        *self.declarations.lock().unwrap_or_else(PoisonError::into_inner) = write.data_sources.0.to_vec();
        *self.placements.lock().unwrap_or_else(PoisonError::into_inner) = write.placements.0.to_vec();

        let mut data_sources_revision = self
            .data_sources_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *data_sources_revision = Some(data_sources_revision.unwrap_or(0) + 1);
        drop(data_sources_revision);

        let mut placements_revision = self
            .placements_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *placements_revision = Some(placements_revision.unwrap_or(0) + 1);

        Ok(())
    }
}

#[tokio::test]
async fn write_environment_says_not_connected_until_something_is() {
    let binding = PlatformDesiredState::unconnected();

    let failure = binding
        .write_environment(
            "lucentroot",
            EnvironmentWrite {
                data_sources: (&[], None),
                placements: (&[], None),
            },
            "Place acme primary",
        )
        .await
        .expect_err("nothing is connected");

    assert_eq!(failure, DesiredStateError::NotConnected);
}

#[tokio::test]
async fn both_revisions_reach_the_adapter_untagged() {
    let a = Repo::new(Some(7), Some(3));

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let declared = binding.read_data_sources("lucentroot").await.expect("A answers");
    let held = binding.read_placements("lucentroot").await.expect("A answers");

    binding
        .write_environment(
            "lucentroot",
            EnvironmentWrite {
                data_sources: (&[], declared.revision.as_ref()),
                placements: (&[placement("acme")], held.revision.as_ref()),
            },
            "Place acme primary",
        )
        .await
        .expect("both decisions are current");

    assert_eq!(a.seen(), vec![(Some("7".to_owned()), Some("3".to_owned()))]);
}

#[tokio::test]
async fn a_rebind_between_the_read_and_the_write_refuses_both_revisions() {
    let a = Repo::new(Some(7), Some(3));
    let b = Repo::new(Some(1), Some(1));

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let declared = binding.read_data_sources("lucentroot").await.expect("A answers");
    let held = binding.read_placements("lucentroot").await.expect("A answers");

    binding
        .connect(Arc::clone(&b) as Arc<dyn PlatformRepository>)
        .await;

    let failure = binding
        .write_environment(
            "lucentroot",
            EnvironmentWrite {
                data_sources: (&[], declared.revision.as_ref()),
                placements: (&[placement("acme")], held.revision.as_ref()),
            },
            "Place acme primary",
        )
        .await
        .expect_err("this decision was taken about a different repository");

    assert_eq!(failure, DesiredStateError::Conflict);
    assert!(
        b.seen().is_empty(),
        "B must not be written to on the strength of A's decision"
    );
}

#[tokio::test]
async fn an_untagged_at_on_either_half_is_refused() {
    let a = Repo::new(Some(7), Some(3));

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let declared = binding.read_data_sources("lucentroot").await.expect("A answers");

    let failure = binding
        .write_environment(
            "lucentroot",
            EnvironmentWrite {
                data_sources: (&[], declared.revision.as_ref()),
                // A bare `None` here carries no generation to check, even
                // though the data-sources half is perfectly current.
                placements: (&[placement("acme")], None),
            },
            "Place acme primary",
        )
        .await
        .expect_err("an untagged placements revision carries no generation to check");

    assert_eq!(failure, DesiredStateError::Conflict);
    assert!(a.seen().is_empty());
}

#[tokio::test]
async fn a_disconnect_waits_for_an_environment_write_already_in_flight() {
    let (a, gate) = Repo::gated(Some(7), Some(3));
    let started = Arc::clone(&a.started);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let declared = binding.read_data_sources("lucentroot").await.expect("A answers");
    let held = binding.read_placements("lucentroot").await.expect("A answers");

    let write = {
        let binding = Arc::clone(&binding);
        tokio::spawn(async move {
            binding
                .write_environment(
                    "lucentroot",
                    EnvironmentWrite {
                        data_sources: (&[], declared.revision.as_ref()),
                        placements: (&[placement("acme")], held.revision.as_ref()),
                    },
                    "Place acme primary",
                )
                .await
        })
    };
    started.notified().await;

    let mut disconnecting = {
        let binding = Arc::clone(&binding);
        tokio::spawn(async move { binding.disconnect().await })
    };

    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut disconnecting)
            .await
            .is_err(),
        "the write is in flight; disconnect must wait for it"
    );

    gate.notify_one();

    write
        .await
        .expect("the write task must not panic")
        .expect("the write lands in the repository it started against");
    disconnecting.await.expect("the disconnect task must not panic");

    assert_eq!(a.seen().len(), 1, "the write landed in A, and only once");
}
