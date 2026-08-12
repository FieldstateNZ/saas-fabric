//! A source backed by a JSON file that reconciliation writes.

use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::resource::{RegistryResource, ResourceSource};
use crate::SourceError;

/// Reads a JSON array of resources from a file.
///
/// # Why a file is a sensible production source
///
/// This looks humble, but it is the shape §6 asks for. A controller reconciles
/// definitions and writes the result into a `ConfigMap` or `Secret`; the
/// platform mounts it; the kubelet keeps the mounted copy current. The runtime
/// then reads a local file, which cannot fail because the API server is busy,
/// and which keeps working when the control plane is down.
///
/// Compare a runtime that watches Kubernetes directly and the trade is clear:
/// this has no control-plane dependency on the request path, and the staleness
/// it costs is bounded by the kubelet sync period plus the refresh interval.
///
/// Each kind of resource gets its own file, so data sources and tenant bindings
/// are reconciled independently — a change to one does not rewrite the other.
pub struct JsonFileSource<T> {
    path: PathBuf,
    resource: PhantomData<fn() -> T>,
}

impl<T> JsonFileSource<T> {
    /// Reads resources from the given path.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            resource: PhantomData,
        }
    }
}

#[async_trait]
impl<T> ResourceSource<T> for JsonFileSource<T>
where
    T: RegistryResource + DeserializeOwned,
{
    async fn load(&self) -> Result<Vec<T>, SourceError> {
        let origin = self.describe();

        // An unreadable file is an error, never an empty set. Returning `Ok`
        // with nothing here would deprovision every resource because a mount
        // was momentarily unavailable.
        let contents = tokio::fs::read(&self.path)
            .await
            .map_err(|cause| SourceError::Unreadable {
                origin: origin.clone(),
                cause: Box::new(cause),
            })?;

        serde_json::from_slice(&contents).map_err(|error| SourceError::Malformed {
            origin,
            detail: error.to_string(),
        })
    }

    fn describe(&self) -> String {
        format!("file:{}", self.path.display())
    }
}
