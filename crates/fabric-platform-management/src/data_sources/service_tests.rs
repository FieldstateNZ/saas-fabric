//! `list` and `declare`, and the difference between a write and a no-op.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, FieldName, IsolationModelDocument, PlacementClassDocument,
    PoolSettingsDocument,
};

use super::{DataSources, Declared};
use crate::{
    ComponentDesired, DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision, DesiredState,
    DesiredStateError, EnvironmentWrite, PlacementRecord, PlacementState, PlacementsRead, PlatformError,
    PlatformRepository,
};

fn declaration(id: &str) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
        revision: BindingRevision::new(0),
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

/// Every write it was asked for, and what it currently holds.
struct Fake {
    declarations: Mutex<Vec<DataSourceDeclaration>>,
    /// `None` models no file yet -- the same absence a fresh environment
    /// reads as.
    revision: Mutex<Option<u64>>,
    writes: Mutex<Vec<Write>>,
}

/// One write, as the fake saw it.
struct Write {
    environment: String,
    declarations: Vec<DataSourceDeclaration>,
    at: Option<String>,
    message: String,
}

impl Fake {
    fn seeded(declarations: Vec<DataSourceDeclaration>, revision: u64) -> Self {
        Self {
            declarations: Mutex::new(declarations),
            revision: Mutex::new(Some(revision)),
            writes: Mutex::new(Vec::new()),
        }
    }

    /// No file declared yet for this environment.
    fn empty() -> Self {
        Self {
            declarations: Mutex::new(Vec::new()),
            revision: Mutex::new(None),
            writes: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait::async_trait]
impl DataSourceState for Fake {
    async fn read_data_sources(&self, _environment: &str) -> Result<DataSourcesRead, DesiredStateError> {
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
        environment: &str,
        declarations: &[DataSourceDeclaration],
        at: Option<&DesiredRevision>,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Write {
                environment: environment.to_owned(),
                declarations: declarations.to_vec(),
                at: at.map(|revision| revision.as_str().to_owned()),
                message: message.to_owned(),
            });

        *self.declarations.lock().unwrap_or_else(PoisonError::into_inner) = declarations.to_vec();
        let mut revision = self.revision.lock().unwrap_or_else(PoisonError::into_inner);
        *revision = Some(revision.unwrap_or(0) + 1);

        Ok(())
    }
}

#[tokio::test]
async fn list_delegates_straight_through() {
    let fake = Arc::new(Fake::seeded(vec![declaration("a")], 3));
    let service = DataSources::new(fake as Arc<dyn DataSourceState>);

    let read = service.list("lucentroot").await.expect("reads");

    assert_eq!(read.declarations.len(), 1);
    assert_eq!(read.declarations[0].id.as_str(), "a");
}

#[tokio::test]
async fn declaring_a_new_id_writes_it_at_revision_one_with_a_declare_message() {
    let fake = Arc::new(Fake::empty());
    let service = DataSources::new(Arc::clone(&fake) as Arc<dyn DataSourceState>);

    let outcome = service
        .declare("lucentroot", declaration("a"), None)
        .await
        .expect("declares");

    let Declared::Written(read) = outcome else {
        panic!("a new declaration must write");
    };
    assert_eq!(read.declarations.len(), 1);
    assert_eq!(read.declarations[0].revision, BindingRevision::new(1));

    let writes = fake.writes.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].environment, "lucentroot");
    assert_eq!(
        writes[0].declarations.len(),
        1,
        "the whole document, not one entry"
    );
    assert!(
        writes[0].message.starts_with("Declare a in lucentroot"),
        "{}",
        writes[0].message
    );
    assert_eq!(writes[0].at, None);
}

#[tokio::test]
async fn declaring_the_same_fields_again_writes_nothing() {
    let fake = Arc::new(Fake::seeded(vec![declaration("a")], 5));
    let service = DataSources::new(Arc::clone(&fake) as Arc<dyn DataSourceState>);

    let outcome = service
        .declare("lucentroot", declaration("a"), Some(&DesiredRevision::new("5")))
        .await
        .expect("declares");

    assert!(matches!(outcome, Declared::Unchanged(_)));
    assert!(fake
        .writes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_empty());
}

#[tokio::test]
async fn correcting_a_held_field_moves_the_revision_forward_with_a_correct_message() {
    let fake = Arc::new(Fake::seeded(vec![declaration("a")], 5));
    let service = DataSources::new(Arc::clone(&fake) as Arc<dyn DataSourceState>);

    let mut changed = declaration("a");
    changed.residency.region = "au".to_owned();

    let outcome = service
        .declare("lucentroot", changed, Some(&DesiredRevision::new("5")))
        .await
        .expect("declares");

    let Declared::Written(read) = outcome else {
        panic!("a changed field must write");
    };
    assert_eq!(read.declarations[0].residency.region, "au");

    let writes = fake.writes.lock().unwrap_or_else(PoisonError::into_inner);
    assert!(
        writes[0].message.starts_with("Correct a in lucentroot"),
        "{}",
        writes[0].message
    );
    assert_eq!(writes[0].at.as_deref(), Some("5"));
}

#[tokio::test]
async fn an_invalid_declaration_is_refused_before_anything_is_read() {
    let fake = Arc::new(Fake::empty());
    let service = DataSources::new(Arc::clone(&fake) as Arc<dyn DataSourceState>);

    let mut invalid = declaration("a");
    invalid.placement = PlacementClassDocument::Shared;
    invalid.discriminator = None;

    let failure = service
        .declare("lucentroot", invalid, None)
        .await
        .expect_err("a shared source without a discriminator is refused");

    assert!(matches!(failure, crate::PlatformError::InvalidDataSource(_)));
    assert!(fake
        .writes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_empty());
}

#[tokio::test]
async fn a_stale_at_is_refused_even_when_the_declaration_is_identical() {
    // The hole this closes: an identical declaration plans as Unchanged,
    // which never reaches write_data_sources -- so a precondition check
    // that lived only there would never see this at all, and a caller
    // whose read is stale would be told 200 instead of the conflict they
    // were owed.
    let fake = Arc::new(Fake::seeded(vec![declaration("a")], 5));
    let service = DataSources::new(Arc::clone(&fake) as Arc<dyn DataSourceState>);

    let failure = service
        .declare("lucentroot", declaration("a"), Some(&DesiredRevision::new("4")))
        .await
        .expect_err("revision 4 was never the held revision");

    assert!(matches!(
        failure,
        crate::PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
    assert!(fake
        .writes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_empty());
}

#[tokio::test]
async fn at_none_is_refused_when_a_file_is_already_present_even_with_an_identical_declaration() {
    let fake = Arc::new(Fake::seeded(vec![declaration("a")], 5));
    let service = DataSources::new(Arc::clone(&fake) as Arc<dyn DataSourceState>);

    let failure = service
        .declare("lucentroot", declaration("a"), None)
        .await
        .expect_err("None means create, and a file is already there");

    assert!(matches!(
        failure,
        crate::PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
    assert!(fake
        .writes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .is_empty());
}

/// A repository that answers every port and lets a test decouple what a
/// read reports from what `write_environment`'s compare-and-swap checks
/// against -- the shape B4's own service-level tests need: a placement
/// recorded between `remove`'s read of placements and its write is a
/// revision `read_placements` already reported, but one `write_environment`
/// no longer accepts.
struct FakeRepository {
    declarations: Mutex<Vec<DataSourceDeclaration>>,
    data_sources_read_revision: Mutex<Option<u64>>,
    data_sources_actual_revision: Mutex<Option<u64>>,
    placements: Mutex<Vec<PlacementRecord>>,
    placements_read_revision: Mutex<Option<u64>>,
    placements_actual_revision: Mutex<Option<u64>>,
    writes: Mutex<Vec<EnvironmentWriteSeen>>,
}

/// One `write_environment` call, as the fake saw it.
struct EnvironmentWriteSeen {
    data_sources: Vec<DataSourceDeclaration>,
    placements: Vec<PlacementRecord>,
}

impl FakeRepository {
    /// A repository whose reads and writes agree -- the ordinary case.
    fn coherent(
        declarations: Vec<DataSourceDeclaration>,
        placements: Vec<PlacementRecord>,
        revision: u64,
    ) -> Self {
        Self {
            declarations: Mutex::new(declarations),
            data_sources_read_revision: Mutex::new(Some(revision)),
            data_sources_actual_revision: Mutex::new(Some(revision)),
            placements: Mutex::new(placements),
            placements_read_revision: Mutex::new(Some(revision)),
            placements_actual_revision: Mutex::new(Some(revision)),
            writes: Mutex::new(Vec::new()),
        }
    }

    /// A repository whose placements moved after this test's read but
    /// before its write -- what `remove` sees is `read_revision`, what its
    /// write is checked against is `actual_revision`.
    fn placements_moved_after_the_read(
        declarations: Vec<DataSourceDeclaration>,
        placements: Vec<PlacementRecord>,
        data_sources_revision: u64,
        read_revision: u64,
        actual_revision: u64,
    ) -> Self {
        Self {
            declarations: Mutex::new(declarations),
            data_sources_read_revision: Mutex::new(Some(data_sources_revision)),
            data_sources_actual_revision: Mutex::new(Some(data_sources_revision)),
            placements: Mutex::new(placements),
            placements_read_revision: Mutex::new(Some(read_revision)),
            placements_actual_revision: Mutex::new(Some(actual_revision)),
            writes: Mutex::new(Vec::new()),
        }
    }

    fn writes(&self) -> Vec<EnvironmentWriteSeen> {
        // `EnvironmentWriteSeen` carries no `Clone`; tests that need this
        // only ever check `.len()` or are the sole owner by then, so a
        // drain suffices and avoids adding a derive nothing else needs.
        let mut writes = self.writes.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *writes)
    }
}

#[async_trait::async_trait]
impl DesiredState for FakeRepository {
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
impl DataSourceState for FakeRepository {
    async fn read_data_sources(&self, _: &str) -> Result<DataSourcesRead, DesiredStateError> {
        Ok(DataSourcesRead {
            revision: (*self
                .data_sources_read_revision
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
impl PlacementState for FakeRepository {
    async fn read_placements(&self, _: &str) -> Result<PlacementsRead, DesiredStateError> {
        Ok(PlacementsRead {
            revision: (*self
                .placements_read_revision
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
impl PlatformRepository for FakeRepository {
    async fn write_environment(
        &self,
        _: &str,
        write: EnvironmentWrite<'_>,
        _: &str,
    ) -> Result<(), DesiredStateError> {
        let expected_data_sources = write.data_sources.1.map(|revision| revision.as_str().to_owned());
        let actual_data_sources = self
            .data_sources_actual_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map(|revision| revision.to_string());
        if expected_data_sources != actual_data_sources {
            return Err(DesiredStateError::Conflict);
        }

        let expected_placements = write.placements.1.map(|revision| revision.as_str().to_owned());
        let actual_placements = self
            .placements_actual_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .map(|revision| revision.to_string());
        if expected_placements != actual_placements {
            return Err(DesiredStateError::Conflict);
        }

        *self.declarations.lock().unwrap_or_else(PoisonError::into_inner) = write.data_sources.0.to_vec();
        *self.placements.lock().unwrap_or_else(PoisonError::into_inner) = write.placements.0.to_vec();

        let mut data_sources_revision = self
            .data_sources_actual_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *data_sources_revision = Some(data_sources_revision.unwrap_or(0) + 1);
        *self
            .data_sources_read_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = *data_sources_revision;
        drop(data_sources_revision);

        let mut placements_revision = self
            .placements_actual_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *placements_revision = Some(placements_revision.unwrap_or(0) + 1);
        *self
            .placements_read_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = *placements_revision;
        drop(placements_revision);

        self.writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(EnvironmentWriteSeen {
                data_sources: write.data_sources.0.to_vec(),
                placements: write.placements.0.to_vec(),
            });

        Ok(())
    }
}

/// The same shape `declaration` builds, but shared -- the class two
/// tenants can legitimately both be recorded placed on, which
/// `removing_a_data_source_a_placement_still_names_is_refused` needs.
fn shared_declaration(id: &str) -> DataSourceDeclaration {
    let mut declared = declaration(id);
    declared.placement = PlacementClassDocument::Shared;
    declared.discriminator = Some(crate::Discriminator {
        column: FieldName::try_new("tenant_key").expect("a valid field name"),
    });
    declared
}

fn placement(tenant: &str, data_source: &str) -> PlacementRecord {
    PlacementRecord {
        tenant: TenantId::try_new(tenant).expect("a valid tenant id"),
        logical: LogicalDataSourceName::try_new("primary").expect("a valid logical data source name"),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new(data_source).expect("a valid data source id"),
        isolation: IsolationModelDocument::Discriminator {
            column: FieldName::try_new("tenant_key").expect("a valid field name"),
            value: tenant.to_owned(),
        },
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    }
}

#[tokio::test]
async fn removing_an_unreferenced_data_source_writes_both_documents_in_one_commit() {
    let repository = FakeRepository::coherent(vec![declaration("a"), declaration("b")], vec![], 5);
    let service = DataSources::new(Arc::new(Fake::seeded(vec![], 0)) as Arc<dyn DataSourceState>);

    let outcome = service
        .remove(
            "lucentroot",
            &DataSourceId::try_new("a").unwrap(),
            Some(&DesiredRevision::new("5")),
            &repository,
        )
        .await
        .expect("removes");

    let Declared::Written(read) = outcome else {
        panic!("removing a declared id must write");
    };
    assert_eq!(read.declarations.len(), 1);
    assert_eq!(read.declarations[0].id.as_str(), "b");

    let writes = repository.writes();
    assert_eq!(writes.len(), 1, "one commit, both documents");
    assert_eq!(writes[0].data_sources.len(), 1);
    assert!(writes[0].placements.is_empty());
}

#[tokio::test]
async fn removing_a_data_source_a_placement_still_names_is_refused() {
    let repository = FakeRepository::coherent(
        vec![shared_declaration("a")],
        vec![placement("acme", "a"), placement("initech", "a")],
        5,
    );
    let service = DataSources::new(Arc::new(Fake::seeded(vec![], 0)) as Arc<dyn DataSourceState>);

    let failure = service
        .remove(
            "lucentroot",
            &DataSourceId::try_new("a").unwrap(),
            Some(&DesiredRevision::new("5")),
            &repository,
        )
        .await
        .expect_err("two tenants still hold placements on it");

    let PlatformError::DataSourceInUse { id, tenants } = failure else {
        panic!("expected DataSourceInUse, got {failure:?}");
    };
    assert_eq!(id.as_str(), "a");
    assert_eq!(tenants.len(), 2);
    assert!(repository.writes().is_empty());
}

#[tokio::test]
async fn removing_an_id_nothing_declares_writes_nothing() {
    let repository = FakeRepository::coherent(vec![declaration("a")], vec![], 5);
    let service = DataSources::new(Arc::new(Fake::seeded(vec![], 0)) as Arc<dyn DataSourceState>);

    let outcome = service
        .remove(
            "lucentroot",
            &DataSourceId::try_new("never-declared").unwrap(),
            Some(&DesiredRevision::new("5")),
            &repository,
        )
        .await
        .expect("removing an absent id is a no-op, not an error");

    assert!(matches!(outcome, Declared::Unchanged(_)));
    assert!(repository.writes().is_empty());
}

#[tokio::test]
async fn removing_the_only_declared_data_source_leaves_an_empty_list() {
    let repository = FakeRepository::coherent(vec![declaration("a")], vec![], 5);
    let service = DataSources::new(Arc::new(Fake::seeded(vec![], 0)) as Arc<dyn DataSourceState>);

    let outcome = service
        .remove(
            "lucentroot",
            &DataSourceId::try_new("a").unwrap(),
            Some(&DesiredRevision::new("5")),
            &repository,
        )
        .await
        .expect("removes the only declaration");

    let Declared::Written(read) = outcome else {
        panic!("removing a declared id must write");
    };
    assert!(read.declarations.is_empty());

    let writes = repository.writes();
    assert_eq!(writes.len(), 1);
    assert!(
        writes[0].data_sources.is_empty(),
        "the file is written empty, not left alone"
    );
}

#[tokio::test]
async fn a_stale_at_is_refused_before_placements_are_even_read() {
    let repository = FakeRepository::coherent(vec![declaration("a")], vec![], 5);
    let service = DataSources::new(Arc::new(Fake::seeded(vec![], 0)) as Arc<dyn DataSourceState>);

    let failure = service
        .remove(
            "lucentroot",
            &DataSourceId::try_new("a").unwrap(),
            Some(&DesiredRevision::new("4")),
            &repository,
        )
        .await
        .expect_err("revision 4 was never the held revision");

    assert!(matches!(
        failure,
        PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
    assert!(repository.writes().is_empty());
}

#[tokio::test]
async fn a_placement_recorded_between_removes_read_and_write_is_a_conflict() {
    // B4: `remove` reads placements, decides nothing is in the way, and
    // only then writes both documents. This fake's `read_placements`
    // reports revision 1 -- what `remove` itself sees and therefore builds
    // its write against -- while `write_environment`'s compare-and-swap is
    // checked against revision 2, standing in for a placement some other
    // caller recorded in the window between the two.
    let repository = FakeRepository::placements_moved_after_the_read(vec![declaration("a")], vec![], 5, 1, 2);
    let service = DataSources::new(Arc::new(Fake::seeded(vec![], 0)) as Arc<dyn DataSourceState>);

    let failure = service
        .remove(
            "lucentroot",
            &DataSourceId::try_new("a").unwrap(),
            Some(&DesiredRevision::new("5")),
            &repository,
        )
        .await
        .expect_err("a placement landed between the read and the write");

    assert!(matches!(
        failure,
        PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
}

#[tokio::test]
async fn data_source_in_use_names_a_tenant_placed_twice_on_it_only_once() {
    // N8: a tenant with `primary` and `audit` both shared on the source
    // being removed is one tenant, not two -- see B2.
    let repository = FakeRepository::coherent(
        vec![shared_declaration("a")],
        vec![
            placement("acme", "a"),
            PlacementRecord {
                logical: LogicalDataSourceName::try_new("audit").unwrap(),
                ..placement("acme", "a")
            },
        ],
        5,
    );
    let service = DataSources::new(Arc::new(Fake::seeded(vec![], 0)) as Arc<dyn DataSourceState>);

    let failure = service
        .remove(
            "lucentroot",
            &DataSourceId::try_new("a").unwrap(),
            Some(&DesiredRevision::new("5")),
            &repository,
        )
        .await
        .expect_err("acme still holds a placement on it");

    let PlatformError::DataSourceInUse { tenants, .. } = failure else {
        panic!("expected DataSourceInUse, got {failure:?}");
    };
    assert_eq!(tenants.len(), 1, "{tenants:?}");
}
