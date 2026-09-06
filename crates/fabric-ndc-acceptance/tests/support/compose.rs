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
use fabric_data_api::{build_data_api, DataApiConfig, ResourceCatalog, ResourcePermissions};
use fabric_identity::{build_identity, IdentityConfig};
use fabric_runtime_publication::{FilesystemRuntimePublication, RuntimePublication as _, RuntimeSnapshot};
use fabric_tenant_runtime::{
    build_runtime, DataSource as RuntimeDataSource, JsonFileSource, RuntimeConfig, TenantRuntimeBinding,
};

use crate::support::tempdir::TempDir;
use crate::support::unsigned_reader::UnsignedTokenReader;

/// The runtime configuration every test in this suite runs under: a long
/// poll interval, since nothing here drives a refresh -- each test brings up
/// its own fixture once and reads it once.
fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        refresh_interval_seconds: 3600,
        fail_fast_on_prime: true,
    }
}

/// Scope checks off -- every test here is about tenancy and connector
/// behaviour, not authorization.
fn open_permissions() -> ResourcePermissions {
    ResourcePermissions {
        require_scopes: false,
        ..ResourcePermissions::default()
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
    let tempdir = TempDir::new();
    let publisher = FilesystemRuntimePublication::new(
        tempdir.tenants_path(),
        tempdir.data_sources_path(),
        tempdir.catalog_path(),
    );
    publisher.publish(snapshot).await.unwrap();

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

    let catalog_bytes = std::fs::read(tempdir.catalog_path()).unwrap();
    let catalog: ResourceCatalog = serde_json::from_slice(&catalog_bytes).unwrap();

    let identity = build_identity(IdentityConfig::default(), Arc::new(UnsignedTokenReader)).unwrap();

    let app = build_data_api(
        &DataApiConfig::default(),
        catalog,
        open_permissions(),
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
