//! Loading the resource catalogue.

use std::path::Path;

use fabric_data_api::ResourceCatalog;
use fabric_tenant_runtime::read_recorded_catalogue;

/// Reads the catalogue that defines which logical resources exist.
///
/// Platform-level and identical for every tenant, so it is a plain file read at
/// startup rather than reconciled state — it changes when the platform's API
/// surface changes, which is a deployment. Where a publication record sits
/// beside the tenants file, the catalogue must be the one it names, so the
/// process never starts on a catalogue from another generation (G8).
///
/// # Errors
///
/// Returns a message if the file cannot be read, is not the recorded
/// generation's catalogue, or is not a valid catalogue.
pub(super) async fn load(path: &Path, tenants_path: &Path) -> Result<ResourceCatalog, String> {
    let contents = read_recorded_catalogue(tenants_path, path)
        .await
        .map_err(|error| {
            format!(
                "could not read the resource catalogue from {}: {error}",
                path.display()
            )
        })?;

    serde_json::from_slice(&contents).map_err(|error| {
        format!(
            "the resource catalogue at {} is malformed: {error}",
            path.display()
        )
    })
}
