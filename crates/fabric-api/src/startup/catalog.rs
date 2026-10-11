//! Loading the resource catalogue.

use std::path::Path;

use fabric_data_api::ResourceCatalog;
use fabric_tenant_runtime::startup_catalogue;

/// Reads the catalogue that defines which logical resources exist.
///
/// Platform-level and identical for every tenant, so it is a plain file read at
/// startup rather than reconciled state — it changes when the platform's API
/// surface changes, which is a deployment. `primed` is the catalogue read with
/// the generation the runtime primed from, so the process never serves a
/// catalogue from another generation than its tenants (G8); see
/// [`startup_catalogue`] for the unprimed case.
///
/// # Errors
///
/// Returns a message if the file cannot be read, is not the recorded
/// generation's catalogue, or is not a valid catalogue.
pub(super) async fn load(
    primed: Option<Vec<u8>>,
    path: &Path,
    tenants_path: &Path,
) -> Result<ResourceCatalog, String> {
    let contents = startup_catalogue(primed, tenants_path, path)
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
