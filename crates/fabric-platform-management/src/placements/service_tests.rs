//! `for_client` previews without writing; `place` writes exactly what
//! `select` decided, guarded by the same precondition `declare` uses, and
//! both operations reparse the client id themselves (ADR 0023 part 2, N11).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use fabric_core::{BindingRevision, Clock, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, FieldName, IsolationModelDocument, PlacementClassDocument,
    PoolSettingsDocument,
};

use super::Placements;
use crate::{
    ComponentDesired, DataIntent, DataSourceDeclaration, DataSourceState, DataSourcesRead, DesiredRevision,
    DesiredState, DesiredStateError, Discriminator, EnvironmentWrite, PlacementOutcome, PlacementRecord,
    PlacementRefusal, PlacementState, PlacementsRead, PlatformError, PlatformRepository,
};

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_787_907_600
    }
}

/// A clock so far outside the calendar range RFC 3339 can spell that
/// formatting it must fail -- N4's proof that a placement is refused
/// rather than recorded with no timestamp.
struct UnformattableClock;

impl Clock for UnformattableClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        u64::MAX
    }
}

fn declaration(id: &str, placement: PlacementClassDocument) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
        revision: BindingRevision::new(1),
        connector: ConnectorId::try_new("postgres-nz").expect("a valid connector id"),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").expect("a valid connection name"),
        },
        placement,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument {
            writable: true,
            accepts_new_tenants: true,
        },
        discriminator: if placement == PlacementClassDocument::Shared {
            Some(Discriminator {
                column: FieldName::try_new("tenant_key").expect("a valid field name"),
            })
        } else {
            None
        },
        labels: BTreeMap::new(),
    }
}

/// A repository that answers every port, records every `write_environment`
/// call, and lets a test decouple what a read reports from what the write
/// is checked against -- see `data_sources::service_tests::FakeRepository`
/// for the same shape, used the same way.
struct FakeRepository {
    declarations: Vec<DataSourceDeclaration>,
    data_sources_read_revision: Option<u64>,
    data_sources_actual_revision: Mutex<Option<u64>>,
    data_sources_reads: Mutex<u32>,
    placements: Mutex<Vec<PlacementRecord>>,
    placements_read_revision: Mutex<Option<u64>>,
    placements_actual_revision: Mutex<Option<u64>>,
    writes: Mutex<Vec<EnvironmentWriteSeen>>,
}

struct EnvironmentWriteSeen {
    data_sources_at: Option<String>,
    placements: Vec<PlacementRecord>,
    placements_at: Option<String>,
    message: String,
}

impl FakeRepository {
    /// A repository whose reads and writes agree, with the given data
    /// sources and no placements yet.
    fn with_data_sources(declarations: Vec<DataSourceDeclaration>) -> Self {
        Self {
            declarations,
            data_sources_read_revision: Some(1),
            data_sources_actual_revision: Mutex::new(Some(1)),
            data_sources_reads: Mutex::new(0),
            placements: Mutex::new(Vec::new()),
            placements_read_revision: Mutex::new(None),
            placements_actual_revision: Mutex::new(None),
            writes: Mutex::new(Vec::new()),
        }
    }

    /// The same, but already holding these placements at this revision.
    fn seeded(
        declarations: Vec<DataSourceDeclaration>,
        placements: Vec<PlacementRecord>,
        revision: u64,
    ) -> Self {
        Self {
            declarations,
            data_sources_read_revision: Some(1),
            data_sources_actual_revision: Mutex::new(Some(1)),
            data_sources_reads: Mutex::new(0),
            placements: Mutex::new(placements),
            placements_read_revision: Mutex::new(Some(revision)),
            placements_actual_revision: Mutex::new(Some(revision)),
            writes: Mutex::new(Vec::new()),
        }
    }

    /// A repository whose data sources moved after this test's read but
    /// before its write: what `place` itself reads is `read_revision`,
    /// what its write is checked against is `actual_revision`.
    fn data_sources_moved_after_the_read(
        declarations: Vec<DataSourceDeclaration>,
        read_revision: u64,
        actual_revision: u64,
    ) -> Self {
        Self {
            declarations,
            data_sources_read_revision: Some(read_revision),
            data_sources_actual_revision: Mutex::new(Some(actual_revision)),
            data_sources_reads: Mutex::new(0),
            placements: Mutex::new(Vec::new()),
            placements_read_revision: Mutex::new(None),
            placements_actual_revision: Mutex::new(None),
            writes: Mutex::new(Vec::new()),
        }
    }

    fn writes(&self) -> Vec<EnvironmentWriteSeen> {
        let mut writes = self.writes.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *writes)
    }

    fn data_sources_reads(&self) -> u32 {
        *self
            .data_sources_reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
        *self
            .data_sources_reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner) += 1;

        Ok(DataSourcesRead {
            revision: self
                .data_sources_read_revision
                .map(|revision| DesiredRevision::new(revision.to_string())),
            declarations: self.declarations.clone(),
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
        message: &str,
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
        let mut placements_revision = self
            .placements_actual_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let actual_placements = placements_revision.map(|revision| revision.to_string());
        if expected_placements != actual_placements {
            return Err(DesiredStateError::Conflict);
        }

        *self.placements.lock().unwrap_or_else(PoisonError::into_inner) = write.placements.0.to_vec();
        *placements_revision = Some(placements_revision.unwrap_or(0) + 1);
        *self
            .placements_read_revision
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = *placements_revision;

        self.writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(EnvironmentWriteSeen {
                data_sources_at: expected_data_sources,
                placements: write.placements.0.to_vec(),
                placements_at: expected_placements,
                message: message.to_owned(),
            });

        Ok(())
    }
}

fn intent(class: PlacementClassDocument) -> DataIntent {
    DataIntent {
        class,
        provider: None,
        region: None,
    }
}

fn service(repository: FakeRepository) -> (Placements, Arc<FakeRepository>) {
    let repository = Arc::new(repository);
    let service = Placements::new(
        Arc::clone(&repository) as Arc<dyn PlatformRepository>,
        Arc::new(FixedClock),
    );
    (service, repository)
}

fn service_with_clock(
    repository: FakeRepository,
    clock: Arc<dyn Clock>,
) -> (Placements, Arc<FakeRepository>) {
    let repository = Arc::new(repository);
    let service = Placements::new(Arc::clone(&repository) as Arc<dyn PlatformRepository>, clock);
    (service, repository)
}

#[tokio::test]
async fn for_client_shows_placeable_when_nothing_is_recorded_and_something_admits_it() {
    let (service, _repository) = service(FakeRepository::with_data_sources(vec![declaration(
        "dedicated-a",
        PlacementClassDocument::Dedicated,
    )]));
    let mut intents = BTreeMap::new();
    intents.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        intent(PlacementClassDocument::Dedicated),
    );

    let result = service
        .for_client("lucentroot", "acme", &intents)
        .await
        .expect("reads");

    let outcome = &result.entries[&LogicalDataSourceName::try_new("primary").unwrap()];
    assert!(matches!(outcome, PlacementOutcome::Placeable));
}

#[tokio::test]
async fn for_client_shows_refused_with_the_reason_when_nothing_admits_it() {
    let (service, _repository) = service(FakeRepository::with_data_sources(vec![]));
    let mut intents = BTreeMap::new();
    intents.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        intent(PlacementClassDocument::Dedicated),
    );

    let result = service
        .for_client("lucentroot", "acme", &intents)
        .await
        .expect("reads");

    let outcome = &result.entries[&LogicalDataSourceName::try_new("primary").unwrap()];
    assert!(matches!(outcome, PlacementOutcome::Refused(_)));
}

#[tokio::test]
async fn for_client_refuses_a_client_id_that_is_not_a_valid_tenant_id() {
    let (service, _repository) = service(FakeRepository::with_data_sources(vec![]));
    let intents = BTreeMap::new();

    let failure = service
        .for_client("lucentroot", "Not A Valid Id", &intents)
        .await
        .expect_err("not a valid tenant id");

    assert!(matches!(
        failure,
        PlatformError::PlacementRefused(PlacementRefusal::TenantIdInvalid { .. })
    ));
}

#[tokio::test]
async fn for_client_shows_placed_for_an_already_recorded_placement_instead_of_running_select() {
    let existing = PlacementRecord {
        tenant: TenantId::try_new("acme").unwrap(),
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        revision: BindingRevision::new(1),
        data_source: DataSourceId::try_new("dedicated-a").unwrap(),
        isolation: IsolationModelDocument::Database {},
        placed_at: "2026-09-18T02:14:00Z".to_owned(),
    };
    let (service, _repository) = service(FakeRepository::seeded(
        vec![declaration("dedicated-a", PlacementClassDocument::Dedicated)],
        vec![existing.clone()],
        3,
    ));
    let mut intents = BTreeMap::new();
    intents.insert(
        LogicalDataSourceName::try_new("primary").unwrap(),
        intent(PlacementClassDocument::Dedicated),
    );

    let result = service
        .for_client("lucentroot", "acme", &intents)
        .await
        .expect("reads");

    let outcome = &result.entries[&LogicalDataSourceName::try_new("primary").unwrap()];
    assert_eq!(outcome, &PlacementOutcome::Placed(existing));
}

#[tokio::test]
async fn place_writes_both_documents_in_one_commit_and_returns_the_new_read() {
    let (service, repository) = service(FakeRepository::with_data_sources(vec![declaration(
        "dedicated-a",
        PlacementClassDocument::Dedicated,
    )]));

    let read = service
        .place(
            "lucentroot",
            "acme",
            &LogicalDataSourceName::try_new("primary").unwrap(),
            &intent(PlacementClassDocument::Dedicated),
            None,
        )
        .await
        .expect("places");

    assert_eq!(read.placements.len(), 1);
    assert_eq!(read.placements[0].data_source.as_str(), "dedicated-a");

    let writes = repository.writes();
    assert_eq!(writes.len(), 1, "one commit, both documents");
    assert!(
        writes[0].message.starts_with("Place acme primary on dedicated-a"),
        "{}",
        writes[0].message
    );
    assert_eq!(writes[0].placements_at, None);
    assert_eq!(
        writes[0].data_sources_at.as_deref(),
        Some("1"),
        "the data-sources revision this call itself read"
    );
    assert_eq!(writes[0].placements.len(), 1, "the whole document, not one entry");
    assert_eq!(writes[0].placements[0].data_source.as_str(), "dedicated-a");
}

#[tokio::test]
async fn place_refuses_a_client_id_that_is_not_a_valid_tenant_id() {
    let (service, repository) = service(FakeRepository::with_data_sources(vec![declaration(
        "dedicated-a",
        PlacementClassDocument::Dedicated,
    )]));

    let failure = service
        .place(
            "lucentroot",
            "Not A Valid Id",
            &LogicalDataSourceName::try_new("primary").unwrap(),
            &intent(PlacementClassDocument::Dedicated),
            None,
        )
        .await
        .expect_err("not a valid tenant id");

    assert!(matches!(
        failure,
        PlatformError::PlacementRefused(PlacementRefusal::TenantIdInvalid { .. })
    ));
    assert!(repository.writes().is_empty());
}

#[tokio::test]
async fn a_stale_at_is_refused_before_select_runs() {
    let (service, repository) = service(FakeRepository::seeded(
        vec![declaration("dedicated-a", PlacementClassDocument::Dedicated)],
        vec![],
        5,
    ));

    let failure = service
        .place(
            "lucentroot",
            "acme",
            &LogicalDataSourceName::try_new("primary").unwrap(),
            &intent(PlacementClassDocument::Dedicated),
            Some(&DesiredRevision::new("4")),
        )
        .await
        .expect_err("revision 4 was never held");

    assert!(matches!(
        failure,
        PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
    assert!(repository.writes().is_empty());
}

#[tokio::test]
async fn a_stale_at_is_refused_before_data_sources_are_even_read() {
    // N9: the precondition is checked immediately after reading
    // placements, before data sources are read at all -- proved here by a
    // repository that would answer `read_data_sources`, but a stale `at`
    // must mean it never gets the chance to.
    let (service, repository) = service(FakeRepository::seeded(
        vec![declaration("dedicated-a", PlacementClassDocument::Dedicated)],
        vec![],
        5,
    ));

    let failure = service
        .place(
            "lucentroot",
            "acme",
            &LogicalDataSourceName::try_new("primary").unwrap(),
            &intent(PlacementClassDocument::Dedicated),
            Some(&DesiredRevision::new("4")),
        )
        .await
        .expect_err("revision 4 was never held");

    assert!(matches!(
        failure,
        PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
    assert_eq!(
        repository.data_sources_reads(),
        0,
        "data sources must not be read before the precondition is checked"
    );
}

#[tokio::test]
async fn a_refusal_from_select_is_not_written() {
    let (service, repository) = service(FakeRepository::with_data_sources(vec![]));

    let failure = service
        .place(
            "lucentroot",
            "acme",
            &LogicalDataSourceName::try_new("primary").unwrap(),
            &intent(PlacementClassDocument::Dedicated),
            None,
        )
        .await
        .expect_err("nothing declared admits this intent");

    assert!(matches!(failure, PlatformError::PlacementRefused(_)));
    assert!(repository.writes().is_empty());
}

#[tokio::test]
async fn a_declaration_changed_between_places_read_and_write_is_a_conflict() {
    // B4: `place` reads data sources itself (after the placements
    // precondition passes) and builds its write from that read. This fake
    // reports revision 1 to that read but checks `write_environment`'s
    // compare-and-swap against revision 2, standing in for a correction
    // `DataSources::declare` landed in the window between the two.
    let (service, _repository) = service(FakeRepository::data_sources_moved_after_the_read(
        vec![declaration("dedicated-a", PlacementClassDocument::Dedicated)],
        1,
        2,
    ));

    let failure = service
        .place(
            "lucentroot",
            "acme",
            &LogicalDataSourceName::try_new("primary").unwrap(),
            &intent(PlacementClassDocument::Dedicated),
            None,
        )
        .await
        .expect_err("a data source declaration landed between the read and the write");

    assert!(matches!(
        failure,
        PlatformError::DesiredState(DesiredStateError::Conflict)
    ));
}

#[tokio::test]
async fn a_clock_that_cannot_be_formatted_refuses_the_placement_rather_than_recording_an_empty_timestamp() {
    let (service, repository) = service_with_clock(
        FakeRepository::with_data_sources(vec![declaration(
            "dedicated-a",
            PlacementClassDocument::Dedicated,
        )]),
        Arc::new(UnformattableClock),
    );

    let failure = service
        .place(
            "lucentroot",
            "acme",
            &LogicalDataSourceName::try_new("primary").unwrap(),
            &intent(PlacementClassDocument::Dedicated),
            None,
        )
        .await
        .expect_err("a clock this far out of range cannot be formatted as RFC 3339");

    assert!(matches!(
        failure,
        PlatformError::DesiredState(DesiredStateError::Unavailable { .. })
    ));
    assert!(repository.writes().is_empty());
}
