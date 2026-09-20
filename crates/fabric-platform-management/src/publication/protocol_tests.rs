//! The offer-and-advance rule, proved against [`FakePublication`] rather
//! than a filesystem.

use fabric_core::{BindingRevision, DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{
    CatalogDocument, DocumentInput, DocumentKind, DocumentOutcome, DocumentRevision, IsolationModelDocument,
    PublicationError, RuntimeSnapshot, TenantBindingDocument, TenantDataBindingDocument, TenantDataBindings,
};

use super::publish_with_retry;
use crate::publication::testing::FakePublication;

fn snapshot(tenants: u64, data_sources: u64, catalog: u64) -> RuntimeSnapshot {
    let mut data = std::collections::BTreeMap::new();
    data.insert(
        LogicalDataSourceName::try_new("primary").expect("valid"),
        TenantDataBindingDocument {
            data_source: DataSourceId::try_new("shared-a").expect("valid"),
            isolation: IsolationModelDocument::Database {},
        },
    );
    let tenant = TenantBindingDocument {
        tenant: TenantId::try_new("acme").expect("valid"),
        revision: BindingRevision::new(1),
        data: TenantDataBindings::try_new(data).expect("non-empty"),
        configuration: None,
        secrets: None,
        features: std::collections::BTreeMap::new(),
        storage: std::collections::BTreeMap::new(),
    };

    RuntimeSnapshot {
        tenants: DocumentInput::new(DocumentRevision::new(tenants), vec![tenant]),
        data_sources: DocumentInput::new(DocumentRevision::new(data_sources), vec![]),
        catalog: DocumentInput::new(
            DocumentRevision::new(catalog),
            CatalogDocument::new(std::collections::BTreeMap::default()),
        ),
    }
}

#[tokio::test]
async fn unchanged_everywhere_writes_nothing_on_a_republish() {
    let target = FakePublication::new();
    publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect("baseline");
    assert_eq!(target.writes().len(), 3);

    let (report, _) = publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect("republish");

    assert_eq!(report.tenants, DocumentOutcome::Unchanged);
    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.catalog, DocumentOutcome::Unchanged);
    assert_eq!(target.writes().len(), 3, "the republish wrote nothing new");
}

#[tokio::test]
async fn one_divergent_document_is_reoffered_once_at_held_plus_one() {
    let target = FakePublication::new();
    publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect("baseline");
    let writes_before = target.writes().len();

    target.script_divergent(DocumentKind::Tenants);
    let (report, offered) = publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect("retries once");

    assert_eq!(offered.tenants.revision, DocumentRevision::new(2));
    assert_eq!(offered.data_sources.revision, DocumentRevision::new(1));
    assert_eq!(offered.catalog.revision, DocumentRevision::new(1));
    assert_eq!(report.tenants, DocumentOutcome::Written);
    assert_eq!(report.data_sources, DocumentOutcome::Unchanged);
    assert_eq!(report.catalog, DocumentOutcome::Unchanged);
    assert_eq!(target.writes().len() - writes_before, 1, "exactly one write");
}

#[tokio::test]
async fn three_divergent_documents_are_each_bumped_once() {
    let target = FakePublication::new();
    publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect("baseline");

    target.script_divergent(DocumentKind::DataSources);
    target.script_divergent(DocumentKind::Catalog);
    target.script_divergent(DocumentKind::Tenants);
    let (report, offered) = publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect("retries three times, once per document");

    assert_eq!(offered.data_sources.revision, DocumentRevision::new(2));
    assert_eq!(offered.catalog.revision, DocumentRevision::new(2));
    assert_eq!(offered.tenants.revision, DocumentRevision::new(2));
    assert_eq!(report.data_sources, DocumentOutcome::Written);
    assert_eq!(report.catalog, DocumentOutcome::Written);
    assert_eq!(report.tenants, DocumentOutcome::Written);
}

#[tokio::test]
async fn a_fourth_divergence_exhausts_the_retry_budget_and_is_refused() {
    let target = FakePublication::new();
    target.script_always_divergent(DocumentKind::DataSources);

    let failure = publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect_err("three retries were not enough");

    assert!(
        matches!(
            failure,
            PublicationError::DivergentPayload {
                document: DocumentKind::DataSources,
                ..
            }
        ),
        "{failure:?}"
    );
    assert!(target.writes().is_empty(), "nothing was ever written");
}

#[tokio::test]
async fn a_stale_revision_refusal_is_returned_immediately_not_retried() {
    let target = FakePublication::new();
    target.script_stale(DocumentKind::Catalog);

    let failure = publish_with_retry(&target, snapshot(1, 1, 1))
        .await
        .expect_err("a stale revision is refused");

    assert!(
        matches!(
            failure,
            PublicationError::StaleRevision {
                document: DocumentKind::Catalog,
                ..
            }
        ),
        "{failure:?}"
    );
    assert!(target.writes().is_empty(), "a stale refusal writes nothing");
}
