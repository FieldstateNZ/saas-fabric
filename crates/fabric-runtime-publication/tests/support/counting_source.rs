//! A `ResourceSource` decorator that counts every `load()` call.
//!
//! `a_failed_refresh_leaves_the_runtime_serving_the_last_good_snapshot` and
//! `a_malformed_published_document_does_not_deprovision_the_tenants_already_serving`
//! simulate a torn mount and want to prove the background refresher actually
//! re-read it. Sampling assertions across a wall-clock window only proves
//! they held for that long -- not that a refresh ever ran during it. Wrapping
//! the real [`JsonFileSource`] with a counter gives those tests a positive
//! signal to poll for instead: the load count advancing.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use fabric_core::RecordDigest;
use fabric_data_api::ResourceCatalogDecoder;
use fabric_tenant_runtime::{
    JsonFileSource, LastKnownGoodState, PublishedFiles, PublishedState, PublishedStateSource,
    RegistryResource, ResourceSource, SourceError,
};
use serde::de::DeserializeOwned;

/// Wraps a real [`JsonFileSource`], counting every `load()` once it has
/// *completed* -- `Err` included, since a failed load is exactly what these
/// tests need to observe, and counting at entry would let a poll return while
/// the load was still in flight.
pub struct CountingSource<T> {
    inner: JsonFileSource<T>,
    loads: Arc<AtomicUsize>,
}

impl<T> CountingSource<T> {
    /// Wraps a file source reading `path`, returning it alongside the shared
    /// counter a test polls.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> (Self, Arc<AtomicUsize>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let source = Self {
            inner: JsonFileSource::new(path),
            loads: Arc::clone(&loads),
        };
        (source, loads)
    }
}

#[async_trait]
impl<T> ResourceSource<T> for CountingSource<T>
where
    T: RegistryResource + DeserializeOwned,
{
    async fn load(&self) -> Result<Vec<T>, SourceError> {
        let result = self.inner.load().await;
        self.loads.fetch_add(1, Ordering::SeqCst);
        result
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}

/// The production reader, [`PublishedFiles`] with the Data API's own
/// catalogue decoder, with the same completed-load counter: one count per
/// read of the three documents and their record. Every other call is
/// forwarded, so the reader behaves exactly as in production.
pub struct CountingPublished {
    inner: PublishedFiles,
    loads: Arc<AtomicUsize>,
}

impl CountingPublished {
    /// Wraps the published files at these three paths, returning the reader
    /// alongside the shared counter a test polls.
    #[must_use]
    pub fn new(
        tenants: impl AsRef<Path>,
        data_sources: impl AsRef<Path>,
        catalog: impl AsRef<Path>,
    ) -> (Self, Arc<AtomicUsize>) {
        Self::over(PublishedFiles::new(
            tenants,
            data_sources,
            catalog,
            Arc::new(ResourceCatalogDecoder),
        ))
    }

    /// Wraps an already configured reader, such as one with a
    /// last-known-good copy.
    #[must_use]
    pub fn over(inner: PublishedFiles) -> (Self, Arc<AtomicUsize>) {
        let loads = Arc::new(AtomicUsize::new(0));
        let source = Self {
            inner,
            loads: Arc::clone(&loads),
        };
        (source, loads)
    }
}

#[async_trait]
impl PublishedStateSource for CountingPublished {
    async fn load(&self) -> Result<PublishedState, SourceError> {
        let result = self.inner.load().await;
        self.loads.fetch_add(1, Ordering::SeqCst);
        result
    }

    async fn load_at_start(&self) -> Result<PublishedState, SourceError> {
        self.inner.load_at_start().await
    }

    async fn installed(&self, generation: u64, record_sha256: &RecordDigest) {
        self.inner.installed(generation, record_sha256).await;
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }

    fn last_known_good(&self) -> LastKnownGoodState {
        self.inner.last_known_good()
    }
}
