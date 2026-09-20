//! Placement reads and writes drain and tag exactly as declared-data-source
//! ones do (`binding/data_sources_tests.rs`) -- these are that file's twins,
//! for the two proofs the build spec calls out by name: a decision read
//! before a rebind is refused after it, and a disconnect waits for a write
//! already in flight. The rest of the set is included too, for the same
//! reason `data_sources_tests.rs` keeps the whole set beside those two: one
//! port going through a mechanism that another port already proved is not
//! proof the wiring is right for this port as well.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use fabric_core::{BindingRevision, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{FieldName, IsolationModelDocument};
use tokio::sync::Notify;

use super::PlatformDesiredState;
use crate::{
    ComponentDesired, DesiredRevision, DesiredState, DesiredStateError, EnvironmentWrite, PlacementRecord,
    PlacementState, PlacementsRead, PlatformRepository,
};

fn placement(tenant: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).expect("a valid tenant id"),
        logical: LogicalDataSourceName::try_new("primary").expect("a valid logical data source name"),
        revision: BindingRevision::new(1),
        data_source: fabric_core::DataSourceId::try_new("shared-a").expect("a valid data source id"),
        isolation: IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").expect("a valid field name"),
            value: tenant.to_owned(),
        },
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

/// A repository that answers every port, recording every placements write
/// and optionally blocking mid-write until a test releases it.
struct Repo {
    name: &'static str,
    placements: Mutex<Vec<PlacementRecord>>,
    /// `None` models no file yet.
    revision: Mutex<Option<u64>>,
    seen: Mutex<Vec<Option<String>>>,
    gate: Option<Arc<Notify>>,
    started: Arc<Notify>,
}

impl Repo {
    fn new(name: &'static str, revision: Option<u64>) -> Arc<Self> {
        Arc::new(Self {
            name,
            placements: Mutex::new(Vec::new()),
            revision: Mutex::new(revision),
            seen: Mutex::new(Vec::new()),
            gate: None,
            started: Arc::new(Notify::new()),
        })
    }

    fn gated(name: &'static str, revision: Option<u64>) -> (Arc<Self>, Arc<Notify>) {
        let gate = Arc::new(Notify::new());

        (
            Arc::new(Self {
                name,
                placements: Mutex::new(Vec::new()),
                revision: Mutex::new(revision),
                seen: Mutex::new(Vec::new()),
                gate: Some(Arc::clone(&gate)),
                started: Arc::new(Notify::new()),
            }),
            gate,
        )
    }

    fn seen(&self) -> Vec<Option<String>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

#[async_trait::async_trait]
impl DesiredState for Repo {
    async fn components(&self, _: &str) -> Result<Vec<String>, DesiredStateError> {
        Ok(vec![self.name.to_owned()])
    }

    async fn component(&self, _: &str, _: &str) -> Result<ComponentDesired, DesiredStateError> {
        Err(DesiredStateError::NotFound {
            what: self.name.to_owned(),
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
impl crate::DataSourceState for Repo {
    async fn read_data_sources(&self, _: &str) -> Result<crate::DataSourcesRead, DesiredStateError> {
        Ok(crate::DataSourcesRead {
            revision: Some(DesiredRevision::new("1")),
            declarations: Vec::new(),
        })
    }

    async fn write_data_sources(
        &self,
        _: &str,
        _: &[crate::DataSourceDeclaration],
        _: Option<&DesiredRevision>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl PlacementState for Repo {
    async fn read_placements(&self, _: &str) -> Result<PlacementsRead, DesiredStateError> {
        let placements = self
            .placements
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let revision = *self.revision.lock().unwrap_or_else(PoisonError::into_inner);

        Ok(PlacementsRead {
            revision: revision.map(|revision| DesiredRevision::new(revision.to_string())),
            placements,
        })
    }

    async fn write_placements(
        &self,
        _: &str,
        placements: &[PlacementRecord],
        at: Option<&DesiredRevision>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(at.map(|revision| revision.as_str().to_owned()));

        self.started.notify_one();

        if let Some(gate) = &self.gate {
            gate.notified().await;
        }

        *self.placements.lock().unwrap_or_else(PoisonError::into_inner) = placements.to_vec();
        let mut revision = self.revision.lock().unwrap_or_else(PoisonError::into_inner);
        *revision = Some(revision.unwrap_or(0) + 1);

        Ok(())
    }
}

/// `PlatformRepository::write_environment` has no blanket implementation
/// (ADR 0023 part 2, B4) -- unused here, `binding/environment_tests.rs` is
/// where it is proved.
#[async_trait::async_trait]
impl PlatformRepository for Repo {
    async fn write_environment(
        &self,
        _: &str,
        _: EnvironmentWrite<'_>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        Ok(())
    }
}

#[tokio::test]
async fn placements_say_not_connected_until_something_is() {
    let binding = PlatformDesiredState::unconnected();

    assert_eq!(
        binding
            .read_placements("lucentroot")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
}

#[tokio::test]
async fn connecting_a_repository_makes_placement_reads_answer() {
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Repo::new("A", Some(7))).await;

    let read = binding.read_placements("lucentroot").await.expect("A answers");
    assert!(read.placements.is_empty());
    assert!(read.revision.is_some());
}

#[tokio::test]
async fn the_adapter_sees_the_revision_it_handed_out_untagged() {
    let a = Repo::new("A", Some(7));

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let read = binding.read_placements("lucentroot").await.expect("A answers");
    let decided = read.revision.expect("a revision to decide against");

    assert_ne!(
        decided.as_str(),
        "7",
        "what leaves the binding carries its generation"
    );

    binding
        .write_placements(
            "lucentroot",
            &[placement("acme")],
            Some(&decided),
            "Place acme primary",
        )
        .await
        .expect("the decision is current");

    assert_eq!(
        a.seen(),
        vec![Some("7".to_owned())],
        "and what reaches the adapter is its own revision, untagged"
    );
}

#[tokio::test]
async fn a_decision_read_before_a_rebind_is_refused_after_it() {
    let a = Repo::new("A", Some(7));
    let b = Repo::new("B", Some(1));

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let decided = binding
        .read_placements("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("a revision");

    binding
        .connect(Arc::clone(&b) as Arc<dyn PlatformRepository>)
        .await;

    let failure = binding
        .write_placements(
            "lucentroot",
            &[placement("acme")],
            Some(&decided),
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
async fn a_create_reaches_the_adapter_untagged_as_none() {
    let a = Repo::new("A", None);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let decided = binding
        .read_placements("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("even an absent file is tagged");

    binding
        .write_placements(
            "lucentroot",
            &[placement("acme")],
            Some(&decided),
            "Place acme primary",
        )
        .await
        .expect("the decision is current");

    assert_eq!(
        a.seen(),
        vec![None],
        "the adapter sees a create -- None -- once the generation checks out"
    );
}

#[tokio::test]
async fn a_create_decided_before_a_rebind_is_refused_after_it() {
    let a = Repo::new("A", None);
    let b = Repo::new("B", None);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let decided = binding
        .read_placements("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("even an absent file is tagged");

    binding
        .connect(Arc::clone(&b) as Arc<dyn PlatformRepository>)
        .await;

    let failure = binding
        .write_placements(
            "lucentroot",
            &[placement("acme")],
            Some(&decided),
            "Place acme primary",
        )
        .await
        .expect_err("this create was decided about a different repository");

    assert_eq!(failure, DesiredStateError::Conflict);
    assert!(
        b.seen().is_empty(),
        "B must not be written to on the strength of a decision taken about A"
    );
}

#[tokio::test]
async fn an_untagged_at_reaching_the_binding_directly_is_refused() {
    let a = Repo::new("A", None);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let failure = binding
        .write_placements("lucentroot", &[placement("acme")], None, "Place acme primary")
        .await
        .expect_err("an untagged None carries no generation to check");

    assert_eq!(failure, DesiredStateError::Conflict);
    assert!(a.seen().is_empty());
}

#[tokio::test]
async fn a_disconnect_waits_for_a_placement_write_already_in_flight() {
    // The proof `binding_tests.rs` gives `DesiredState` and
    // `data_sources_tests.rs` gives `DataSourceState`, pinned for this port
    // too: a write already running against A must finish against A before
    // a disconnect can return.
    let (a, gate) = Repo::gated("A", Some(7));
    let started = Arc::clone(&a.started);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let decided = binding
        .read_placements("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("a revision");

    let write = {
        let binding = Arc::clone(&binding);
        tokio::spawn(async move {
            binding
                .write_placements(
                    "lucentroot",
                    &[placement("acme")],
                    Some(&decided),
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
    assert_eq!(
        binding
            .read_placements("lucentroot")
            .await
            .expect_err("nothing is connected any more"),
        DesiredStateError::NotConnected
    );
}
