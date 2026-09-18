//! `list` and `declare`, and the difference between a write and a no-op.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use fabric_core::{BindingRevision, DataSourceId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, PlacementClassDocument, PoolSettingsDocument,
};

use super::{DataSources, Declared};
use crate::{DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision, DesiredStateError};

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
