//! Declared-data-source reads and writes drain and tag exactly as a
//! component's desired state does -- see `binding_tests.rs` for the fuller
//! proofs of the drain itself; this file pins that `DataSourceState` goes
//! through the identical mechanism, and that a create is generation-safe
//! too (ADR 0023 fix pass, B3).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use fabric_core::{BindingRevision, DataSourceId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, PlacementClassDocument, PoolSettingsDocument,
};
use tokio::sync::Notify;

use super::PlatformDesiredState;
use crate::{
    ComponentDesired, DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision, DesiredState,
    DesiredStateError, EnvironmentWrite, PlacementRecord, PlacementState, PlacementsRead, PlatformRepository,
};

fn declaration(id: &str) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").expect("a valid connector id"),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").expect("a valid connection name"),
        },
        placement: PlacementClassDocument::Dedicated,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        discriminator: None,
        labels: BTreeMap::new(),
    }
}

/// A repository that answers both ports, recording every write and
/// optionally blocking mid-write until a test releases it.
struct Repo {
    name: &'static str,
    declarations: Mutex<Vec<DataSourceDeclaration>>,
    /// None models no file yet -- the same absence a fresh environment
    /// reads as.
    revision: Mutex<Option<u64>>,
    seen: Mutex<Vec<Option<String>>>,
    /// Held open until a test releases it, when there is one.
    gate: Option<Arc<Notify>>,
    /// Signalled once a write has genuinely started.
    started: Arc<Notify>,
}

impl Repo {
    fn new(name: &'static str, revision: Option<u64>) -> Arc<Self> {
        Arc::new(Self {
            name,
            declarations: Mutex::new(Vec::new()),
            revision: Mutex::new(revision),
            seen: Mutex::new(Vec::new()),
            gate: None,
            started: Arc::new(Notify::new()),
        })
    }

    /// A repository whose write blocks until the returned gate is released.
    fn gated(name: &'static str, revision: Option<u64>) -> (Arc<Self>, Arc<Notify>) {
        let gate = Arc::new(Notify::new());

        (
            Arc::new(Self {
                name,
                declarations: Mutex::new(Vec::new()),
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
impl DataSourceState for Repo {
    async fn read_data_sources(&self, _: &str) -> Result<DataSourcesRead, DesiredStateError> {
        let declarations = self
            .declarations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let revision = *self.revision.lock().unwrap_or_else(PoisonError::into_inner);

        Ok(DataSourcesRead {
            revision: revision.map(|revision| DesiredRevision::new(revision.to_string())),
            declarations,
        })
    }

    async fn write_data_sources(
        &self,
        _: &str,
        declarations: &[DataSourceDeclaration],
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

        *self.declarations.lock().unwrap_or_else(PoisonError::into_inner) = declarations.to_vec();
        let mut revision = self.revision.lock().unwrap_or_else(PoisonError::into_inner);
        *revision = Some(revision.unwrap_or(0) + 1);

        Ok(())
    }
}

/// `PlatformDesiredState::connect` requires `Arc<dyn PlatformRepository>`
/// (ADR 0023 part 2 added `PlacementState` to it) -- nothing in this file
/// exercises this half of `Repo`, so it answers a fixed empty read and
/// does not record the write. `binding/placements_tests.rs` is where this
/// port's own generation-tagging is proved.
#[async_trait::async_trait]
impl PlacementState for Repo {
    async fn read_placements(&self, _: &str) -> Result<PlacementsRead, DesiredStateError> {
        Ok(PlacementsRead {
            revision: Some(DesiredRevision::new("1")),
            placements: Vec::new(),
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
async fn data_sources_say_not_connected_until_something_is() {
    let binding = PlatformDesiredState::unconnected();

    assert_eq!(
        binding
            .read_data_sources("lucentroot")
            .await
            .expect_err("nothing is connected"),
        DesiredStateError::NotConnected
    );
}

#[tokio::test]
async fn connecting_a_repository_makes_data_source_reads_answer() {
    let binding = PlatformDesiredState::unconnected();
    binding.connect(Repo::new("A", Some(7))).await;

    let read = binding.read_data_sources("lucentroot").await.expect("A answers");
    assert!(read.declarations.is_empty());
    assert!(read.revision.is_some());
}

#[tokio::test]
async fn the_adapter_sees_the_revision_it_handed_out_untagged() {
    let a = Repo::new("A", Some(7));

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let read = binding.read_data_sources("lucentroot").await.expect("A answers");
    let decided = read.revision.expect("a revision to decide against");

    assert_ne!(
        decided.as_str(),
        "7",
        "what leaves the binding carries its generation"
    );

    binding
        .write_data_sources("lucentroot", &[declaration("x")], Some(&decided), "Declare x")
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
        .read_data_sources("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("a revision");

    binding
        .connect(Arc::clone(&b) as Arc<dyn PlatformRepository>)
        .await;

    let failure = binding
        .write_data_sources("lucentroot", &[declaration("x")], Some(&decided), "Declare x")
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
    // B3: a create decided against a repository still has to carry that
    // repository generation, exactly as a replace does -- so this reads
    // first, and decides from what that read handed back, rather than
    // building an untagged at by hand the way the pre-fix behaviour did.
    let a = Repo::new("A", None);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let decided = binding
        .read_data_sources("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("even an absent file is tagged");

    binding
        .write_data_sources("lucentroot", &[declaration("x")], Some(&decided), "Declare x")
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
    // The failure B3 exists to close: a create decided while nothing was
    // declared in A must not land in B just because B also has nothing
    // declared -- the decision was never taken about B.
    let a = Repo::new("A", None);
    let b = Repo::new("B", None);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let decided = binding
        .read_data_sources("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("even an absent file is tagged");

    binding
        .connect(Arc::clone(&b) as Arc<dyn PlatformRepository>)
        .await;

    let failure = binding
        .write_data_sources("lucentroot", &[declaration("x")], Some(&decided), "Declare x")
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
    // A bare None carries no generation at all, so it cannot be trusted
    // even when something is connected and even when the file really is
    // absent -- the binding only ever hands back a tagged token, and a
    // caller that bypassed reading one has nothing valid to present.
    let a = Repo::new("A", None);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let failure = binding
        .write_data_sources("lucentroot", &[declaration("x")], None, "Declare x")
        .await
        .expect_err("an untagged None carries no generation to check");

    assert_eq!(failure, DesiredStateError::Conflict);
    assert!(a.seen().is_empty());
}

#[tokio::test]
async fn a_disconnect_waits_for_a_data_source_write_already_in_flight() {
    // The same drain binding_tests.rs proves for DesiredState, pinned for
    // this port too: a write already running against A must finish
    // against A before a disconnect can return.
    let (a, gate) = Repo::gated("A", Some(7));
    let started = Arc::clone(&a.started);

    let binding = PlatformDesiredState::unconnected();
    binding
        .connect(Arc::clone(&a) as Arc<dyn PlatformRepository>)
        .await;

    let decided = binding
        .read_data_sources("lucentroot")
        .await
        .expect("A answers")
        .revision
        .expect("a revision");

    let write = {
        let binding = Arc::clone(&binding);
        tokio::spawn(async move {
            binding
                .write_data_sources("lucentroot", &[declaration("x")], Some(&decided), "Declare x")
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
            .read_data_sources("lucentroot")
            .await
            .expect_err("nothing is connected any more"),
        DesiredStateError::NotConnected
    );
}
