//! A binding source backed by a file that reconciliation writes.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::{BindingSource, BindingSourceError, TenantRuntimeBinding};

/// Reads bindings from a JSON file.
///
/// # Why a file is a sensible production source
///
/// This looks humble, but it is the shape §6 asks for. A controller reconciles
/// tenant definitions and writes the resulting bindings into a `ConfigMap` or
/// `Secret`; the platform mounts it; the kubelet keeps the mounted copy current.
/// The runtime then reads a local file, which cannot fail because the API
/// server is busy, and which keeps working when the control plane is down.
///
/// Compare the alternative — a runtime that watches Kubernetes directly — and
/// the trade is clear: this one has no control-plane dependency on the request
/// path at all, and the staleness it costs is bounded by the kubelet's sync
/// period plus the refresh interval.
///
/// # Format
///
/// A JSON array of [`TenantRuntimeBinding`]:
///
/// ```json
/// [
///   {
///     "tenant": "acme",
///     "revision": 42,
///     "data": {
///       "primary": {
///         "connector": "postgres-au-east",
///         "connection": {"kind": "named", "name": "acme-prod"},
///         "isolation": {"kind": "database"}
///       }
///     }
///   }
/// ]
/// ```
pub struct FileBindingSource {
    path: PathBuf,
}

impl FileBindingSource {
    /// Reads bindings from the given path.
    #[must_use]
    pub fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl BindingSource for FileBindingSource {
    async fn load(&self) -> Result<Vec<TenantRuntimeBinding>, BindingSourceError> {
        let describe = self.describe();

        // An unreadable file is an error, never an empty set. Returning `Ok`
        // with no bindings here would deprovision every tenant because a
        // mount was momentarily unavailable.
        let contents = tokio::fs::read(&self.path)
            .await
            .map_err(|cause| BindingSourceError::Unreadable {
                origin: describe.clone(),
                cause: Box::new(cause),
            })?;

        serde_json::from_slice(&contents).map_err(|error| BindingSourceError::Malformed {
            origin: describe,
            detail: error.to_string(),
        })
    }

    fn describe(&self) -> String {
        format!("file:{}", self.path.display())
    }
}

#[cfg(test)]
mod tests {
    use fabric_core::BindingRevision;

    use super::*;

    /// Writes a file into a fresh directory under the process temp dir and
    /// returns its path. Kept tiny on purpose — pulling in a temp-file crate
    /// for three tests is not worth the dependency.
    fn write_temp(name: &str, contents: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("fabric-binding-source-{name}"));
        std::fs::create_dir_all(&directory).unwrap();

        let path = directory.join("bindings.json");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[tokio::test]
    async fn reads_bindings_from_a_json_array() {
        let path = write_temp(
            "valid",
            r#"[
                {
                    "tenant": "acme",
                    "revision": 42,
                    "data": {
                        "primary": {
                            "connector": "postgres-au-east",
                            "connection": {"kind": "named", "name": "acme-prod"},
                            "isolation": {"kind": "database"}
                        }
                    }
                }
            ]"#,
        );

        let bindings = FileBindingSource::new(&path).load().await.unwrap();

        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.first().unwrap().revision, BindingRevision::new(42));
    }

    #[tokio::test]
    async fn an_empty_array_is_a_legitimate_empty_set() {
        let path = write_temp("empty", "[]");

        assert!(FileBindingSource::new(&path).load().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_missing_file_is_an_error_not_an_empty_set() {
        // This is the important one: an unreadable mount must not be allowed to
        // deprovision every tenant.
        let source = FileBindingSource::new("/nonexistent/fabric/bindings.json");

        assert!(matches!(
            source.load().await.unwrap_err(),
            BindingSourceError::Unreadable { .. }
        ));
    }

    #[tokio::test]
    async fn malformed_json_is_an_error_not_an_empty_set() {
        let path = write_temp("malformed", "{ not json");

        assert!(matches!(
            FileBindingSource::new(&path).load().await.unwrap_err(),
            BindingSourceError::Malformed { .. }
        ));
    }

    #[tokio::test]
    async fn a_binding_with_an_invalid_tenant_id_is_rejected_at_the_boundary() {
        let path = write_temp("bad-tenant", r#"[{"tenant": "Acme Corp", "revision": 1}]"#);

        assert!(matches!(
            FileBindingSource::new(&path).load().await.unwrap_err(),
            BindingSourceError::Malformed { .. }
        ));
    }
}
