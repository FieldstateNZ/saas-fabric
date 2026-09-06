//! Shared fixtures for the composed acceptance test.
//!
//! One place for the temporary directory, the recording connector, the
//! tenant/DataSource/catalogue fixture, and the stack builder every test in
//! `published_state_serves_two_tenants.rs` drives. Not every test uses every
//! export, so unused-item warnings are allowed here rather than at each call
//! site -- the same convention `fabric-data-api`'s own `tests/support/mod.rs`
//! uses.

// The crate's atomic-write guarantee is Unix-only, and so is this proof of it.
#![cfg(unix)]
#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

mod connector;
mod counting_source;
mod fixtures;
mod requests;

pub use connector::RecordingConnector;
pub use counting_source::CountingSource;
pub use fixtures::{
    articles_catalog, base_snapshot, data_source_id, shared_data_source, tenant_binding,
    ACME_DISCRIMINATOR_VALUE, CONNECTOR_ID, DATA_SOURCE_ID, DISCRIMINATOR_COLUMN, GLOBEX_DISCRIMINATOR_VALUE,
};
pub use requests::{body_json, claims_for, request, request_with_tenant_header};

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::response::Response;
use axum::Router;
use fabric_data_api::{build_data_api, DataApiConfig, ResourceCatalog, ResourcePermissions};
use fabric_identity::{build_identity, IdentityConfig, TrustedIngressReader};
use fabric_runtime_publication::{FilesystemRuntimePublication, RuntimePublication as _, RuntimeSnapshot};
use fabric_tenant_runtime::{
    build_runtime, DataSource as RuntimeDataSource, RuntimeConfig, RuntimeHandles, RuntimeResolver,
    TenantRuntimeBinding,
};
use http::Request;
use tower::ServiceExt as _;

/// A validated connector-side field name (`fabric_connector::FieldName`) --
/// never this crate's own re-declared `FieldName`, which only ever appears
/// inside a published document. Kept as one helper so the two never get
/// confused at a call site.
pub fn field(name: &str) -> fabric_connector::FieldName {
    fabric_connector::FieldName::try_new(name).unwrap()
}

/// A clock frozen so unsigned test tokens never expire -- the same posture
/// `fabric-data-api`'s own composed tests run under.
pub struct FixedClock;

impl fabric_core::Clock for FixedClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }

    fn now_unix_seconds(&self) -> u64 {
        1_000
    }
}

/// A directory under the system temp root, unique per test, removed when it
/// drops. `tempfile` is not in this workspace's dependency table, so this is
/// the whole of what that crate would otherwise give us -- the same pattern
/// `tests/filesystem_runtime_publication.rs` already uses.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a fresh, empty directory.
    ///
    /// The name mixes in a process-wide counter, not only the clock: every
    /// test in this suite that touches the runtime plane spawns background
    /// tasks, so `cargo test`'s default parallelism means several
    /// `TempDir::new()` calls can land in the same nanosecond on a clock
    /// whose actual resolution is coarser than that. Two tests racing onto
    /// one directory is worse than a slow test -- one test's `Drop` deletes
    /// the files the other is still publishing into, which reads as this
    /// crate's own guards failing rather than as what it is: a fixture bug.
    #[must_use]
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        let unique = format!(
            "fabric-runtime-publication-composed-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    /// The directory itself.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where a publisher would write the tenants payload.
    #[must_use]
    pub fn tenants_path(&self) -> PathBuf {
        self.path.join("tenants.json")
    }

    /// Where a publisher would write the data-sources payload.
    #[must_use]
    pub fn data_sources_path(&self) -> PathBuf {
        self.path.join("data-sources.json")
    }

    /// Where a publisher would write the catalogue payload.
    #[must_use]
    pub fn catalog_path(&self) -> PathBuf {
        self.path.join("catalog.json")
    }

    /// A fresh [`FilesystemRuntimePublication`] over this directory's three
    /// paths. Stateless, so reconstructing one is exactly as valid as
    /// reusing whichever instance a helper already built.
    #[must_use]
    pub fn publisher(&self) -> FilesystemRuntimePublication {
        FilesystemRuntimePublication::new(self.tenants_path(), self.data_sources_path(), self.catalog_path())
    }

    /// Overwrites a file in this directory with raw bytes, bypassing the
    /// publisher entirely. Used to simulate a consumer-side failure -- bytes
    /// that arrived some other way than a publication, such as a torn mount
    /// -- never a publication ADR 0018's own guards would have refused.
    pub fn write_raw(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.path.join(name), bytes).unwrap();
    }
}

impl Default for TempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// `(inode, modified-time in nanoseconds)`, so a rewrite is detectable even
/// within the same clock tick as the read it is compared against.
#[must_use]
pub fn file_identity(path: &Path) -> (u64, i64) {
    let metadata = std::fs::metadata(path).unwrap();
    (metadata.ino(), metadata.mtime_nsec())
}

/// The runtime configuration every test in this suite runs under: a long
/// poll interval, because every test drives a refresh explicitly with
/// `refresh_now` and a short interval would let the periodic loop interleave
/// with what a test is trying to observe deterministically.
#[must_use]
pub fn runtime_config() -> RuntimeConfig {
    RuntimeConfig {
        refresh_interval_seconds: 3600,
        fail_fast_on_prime: true,
    }
}

/// Scope checks off -- every test here is about tenancy, not authorization.
#[must_use]
pub fn open_permissions() -> ResourcePermissions {
    ResourcePermissions {
        require_scopes: false,
        ..ResourcePermissions::default()
    }
}

/// Everything one test needs: the temp directory the files were published
/// into, the real runtime built from them, the real Data API router in
/// front of it, and the connector that recorded what reached it.
pub struct Stack {
    pub app: Router,
    pub resolver: Arc<RuntimeResolver>,
    pub handles: RuntimeHandles,
    pub connector: Arc<RecordingConnector>,
    pub dir: TempDir,
    /// How many times the tenants source's `load()` has been called --
    /// counted through [`CountingSource`], so a test can prove a refresh
    /// actually ran rather than only that assertions held across a window.
    pub tenant_loads: Arc<AtomicUsize>,
    /// The data-sources twin of `tenant_loads`.
    pub data_source_loads: Arc<AtomicUsize>,
}

/// Publishes `snapshot` through the real [`FilesystemRuntimePublication`]
/// into a fresh temporary directory, then builds the real
/// `fabric_tenant_runtime::build_runtime` (over [`CountingSource`], which
/// delegates every call to a real `fabric_tenant_runtime::JsonFileSource`)
/// and the real `fabric_data_api::build_data_api` router over exactly the
/// files that publication wrote -- the composed stack every test in this
/// suite drives.
///
/// The catalogue leg cannot reuse `fabric-api`'s own `startup::catalog::load`
/// (it is `pub(super)`, per ADR 0018's Consequences), so this reads
/// `catalog.json` back and deserialises it as the real, public
/// `fabric_data_api::ResourceCatalog` -- the same type `load` builds --
/// which is what pins the catalogue leg the way the compiler already pins
/// the tenants and data-sources legs through `JsonFileSource`.
pub async fn build_stack(snapshot: &RuntimeSnapshot) -> Stack {
    let dir = TempDir::new();
    dir.publisher().publish(snapshot).await.unwrap();

    let (tenant_source, tenant_loads) = CountingSource::<TenantRuntimeBinding>::new(dir.tenants_path());
    let (data_source_source, data_source_loads) =
        CountingSource::<RuntimeDataSource>::new(dir.data_sources_path());

    let (resolver, handles) = build_runtime(
        &runtime_config(),
        Arc::new(tenant_source),
        Arc::new(data_source_source),
    )
    .await
    .unwrap();

    let catalog_bytes = std::fs::read(dir.catalog_path()).unwrap();
    let catalog: ResourceCatalog = serde_json::from_slice(&catalog_bytes).unwrap();

    let connector = RecordingConnector::new();

    // The issuer registry is required configuration (ADR 0019 §2), and this
    // suite drives two tenants through one resolver, so each gets its own.
    let identity = build_identity(
        IdentityConfig {
            trusted_issuers: requests::trusted_issuers(),
            ..IdentityConfig::default()
        },
        Arc::new(TrustedIngressReader::new(Arc::new(FixedClock))),
    )
    .unwrap();

    let app = build_data_api(
        &DataApiConfig::default(),
        catalog,
        open_permissions(),
        Arc::clone(&resolver),
        fabric_connector::ConnectorRegistry::new()
            .with(Arc::clone(&connector) as Arc<dyn fabric_connector::DataConnector>),
        identity,
    )
    .unwrap();

    Stack {
        app,
        resolver,
        handles,
        connector,
        dir,
        tenant_loads,
        data_source_loads,
    }
}

/// Polls `count` until it advances past `baseline`, or fails the test once
/// `deadline` has elapsed without that happening.
///
/// `refresh_now` only notifies the background refresher; a failed load never
/// touches the registry either, so there is no positive signal to await
/// beyond the source having actually been asked to load again. This is that
/// signal -- proof a refresh ran, not just that assertions held across a
/// wall-clock window.
pub async fn poll_for_load_count_above(count: &AtomicUsize, baseline: usize, deadline: Duration) {
    let start = Instant::now();
    loop {
        if count.load(Ordering::SeqCst) > baseline {
            return;
        }
        assert!(
            start.elapsed() < deadline,
            "load count did not advance past {baseline} within {deadline:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Repeatedly sends a fresh request built by `build_request` until the
/// response's status matches `expected`, or fails the test once `deadline`
/// has elapsed without it doing so.
///
/// `refresh_now` only notifies the background refresher; this is how a test
/// waits for that refresh to have actually taken effect without one
/// arbitrary sleep deciding the outcome.
pub async fn poll_for_status(
    app: &Router,
    expected: http::StatusCode,
    build_request: impl Fn() -> Request<Body>,
    deadline: Duration,
) -> Response {
    let start = Instant::now();
    loop {
        let response = app.clone().oneshot(build_request()).await.unwrap();
        if response.status() == expected {
            return response;
        }
        assert!(
            start.elapsed() < deadline,
            "expected {expected} within {deadline:?}, last saw {}",
            response.status()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
