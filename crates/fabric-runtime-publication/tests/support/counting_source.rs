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
use fabric_tenant_runtime::{JsonFileSource, RegistryResource, ResourceSource, SourceError};
use serde::de::DeserializeOwned;

/// Wraps a real [`JsonFileSource`], counting every call to `load()` -- the
/// `Err` path included, since a failed load is exactly what these tests need
/// to observe.
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
        self.loads.fetch_add(1, Ordering::SeqCst);
        self.inner.load().await
    }

    fn describe(&self) -> String {
        self.inner.describe()
    }
}
