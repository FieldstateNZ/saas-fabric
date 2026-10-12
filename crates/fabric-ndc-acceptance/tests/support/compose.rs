//! Publishing a fixture and assembling the real consumer stack over it.
//!
//! [`compose`] is the one place this crate's tests build the full chain
//! issue #62 exists to prove: the real `FilesystemRuntimePublication`, the
//! real `fabric_tenant_runtime::build_runtime`, and the real
//! `fabric_data_api::build_data_api`, wired to a connector the caller has
//! already negotiated against a running container (`support::stack::Stack`
//! or a raw `build_ndc_connector` call, depending on the test). Splitting
//! connector negotiation out of this function is deliberate: several tests
//! need the negotiation itself to fail, which [`compose`] cannot express.

use std::sync::Arc;

use axum::Router;
use fabric_connector::{ConnectorRegistry, DataConnector};
use fabric_connector_ndc::NdcConnector;
use fabric_data_api::{build_data_api, DataApiConfig, DataAuthorization, ResourceCatalog};
use fabric_identity::{build_identity, IdentityConfig};
use fabric_runtime_publication::{
    CatalogDocument, FilesystemRuntimePublication, PublicationError, RuntimePublication as _, RuntimeSnapshot,
};
use fabric_tenant_runtime::{
    build_runtime, DataSource as RuntimeDataSource, DecodedCatalogue, JsonFileSource, RuntimeConfig,
    TenantRuntimeBinding,
};

use crate::support::requests;
use crate::support::tempdir::TempDir;
use crate::support::unsigned_reader::UnsignedTokenReader;

/// The runtime configuration every test in this suite runs under: a long
/// poll interval, since nothing here drives a refresh -- each test brings up
/// its own fixture once and reads it once.
fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        refresh_interval_seconds: 3600,
        fail_fast_on_prime: true,
        ..RuntimeConfig::default()
    }
}

/// Everything [`compose`] hands back: the real Data API router, and the
/// directory it was published into -- kept alive for the test's duration
/// even though nothing re-reads it, so a reader is never left wondering why
/// the published files disappeared mid-test.
pub struct Composed {
    /// The assembled router: publisher-written files, the real runtime, and
    /// the real Data API in front of the connector `compose` was given.
    pub app: Router,
    _tempdir: TempDir,
}

/// Publishes `snapshot` through the real [`FilesystemRuntimePublication`]
/// into a fresh temporary directory, builds the real
/// `fabric_tenant_runtime::build_runtime` over the real `JsonFileSource`s on
/// the files that publication wrote, and serves them through the real
/// `fabric_data_api::build_data_api` router with `connector` registered
/// under [`crate::support::fixtures::CONNECTOR_ID`].
///
/// `connector` must already be negotiated -- built by a prior, successful
/// `fabric_connector_ndc::build_ndc_connector` call -- because a negotiation
/// failure is exactly what several tests in this suite need to observe
/// directly, which composing it away here would hide.
pub async fn compose(connector: Arc<NdcConnector>, snapshot: &RuntimeSnapshot) -> Composed {
    compose_published(connector, std::slice::from_ref(snapshot)).await
}

/// [`compose`] over several publications made in order, the runtime built
/// over whatever the last one left on disk -- the way a catalogue published
/// ahead of the tenants document it depends on reaches a consumer.
pub async fn compose_published(connector: Arc<NdcConnector>, snapshots: &[RuntimeSnapshot]) -> Composed {
    compose_over(connector, snapshots, None).await
}

/// [`compose`] over `snapshot`, with `catalog.json` then overwritten by
/// `catalog` outside the publisher: the way a catalogue the publisher
/// refuses can still reach a runtime that reads it without the publication
/// record, as a hand edit or a volume from another generation.
pub async fn compose_with_catalogue_written_outside_the_publisher(
    connector: Arc<NdcConnector>,
    snapshot: &RuntimeSnapshot,
    catalog: &CatalogDocument,
) -> Composed {
    compose_over(connector, std::slice::from_ref(snapshot), Some(catalog)).await
}

/// What the real publisher answers for `snapshots`, published in order into
/// a fresh directory: the first refusal, which the caller expects.
pub async fn first_refusal(snapshots: &[RuntimeSnapshot]) -> PublicationError {
    let tempdir = TempDir::new();
    let publisher = FilesystemRuntimePublication::new(
        tempdir.tenants_path(),
        tempdir.data_sources_path(),
        tempdir.catalog_path(),
    );
    for snapshot in snapshots {
        if let Err(refusal) = publisher.publish(snapshot).await {
            return refusal;
        }
    }
    panic!("every publication was accepted")
}

async fn compose_over(
    connector: Arc<NdcConnector>,
    snapshots: &[RuntimeSnapshot],
    catalog: Option<&CatalogDocument>,
) -> Composed {
    let tempdir = TempDir::new();
    let publisher = FilesystemRuntimePublication::new(
        tempdir.tenants_path(),
        tempdir.data_sources_path(),
        tempdir.catalog_path(),
    );
    for snapshot in snapshots {
        publisher.publish(snapshot).await.unwrap();
    }
    if let Some(catalog) = catalog {
        std::fs::write(tempdir.catalog_path(), catalog.canonical_json().unwrap()).unwrap();
    }

    let (resolver, _handles) = build_runtime(
        &runtime_config(),
        Arc::new(JsonFileSource::<TenantRuntimeBinding>::new(
            tempdir.tenants_path(),
        )),
        Arc::new(JsonFileSource::<RuntimeDataSource>::new(
            tempdir.data_sources_path(),
        )),
    )
    .await
    .unwrap();

    // The reader before the record: one catalogue, read once at start, and
    // served from every view of this resolver.
    let catalog_bytes = std::fs::read(tempdir.catalog_path()).unwrap();
    let catalog: ResourceCatalog = serde_json::from_slice(&catalog_bytes).unwrap();
    let resolver = Arc::new(resolver.with_catalogue(DecodedCatalogue::new(catalog)));

    // A runtime that trusts no issuer refuses to start (ADR 0019 §1), so the
    // composed one registers an issuer for each of the two tenants whose rows
    // share the table; `requests::claims_for` mints every token against it.
    let identity_config = IdentityConfig {
        trusted_issuers: requests::trusted_issuers(),
        ..IdentityConfig::default()
    };
    let identity = build_identity(identity_config, Arc::new(UnsignedTokenReader)).unwrap();

    let app = build_data_api(
        &DataApiConfig::default(),
        DataAuthorization::default(),
        resolver,
        ConnectorRegistry::new().with(connector as Arc<dyn DataConnector>),
        identity,
    )
    .unwrap();

    Composed {
        app,
        _tempdir: tempdir,
    }
}
