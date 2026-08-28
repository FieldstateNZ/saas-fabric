//! Loading the resource catalogue.

use std::path::Path;

use fabric_data_api::ResourceCatalog;

/// Reads the catalogue that defines which logical resources exist.
///
/// Platform-level and identical for every tenant, so it is a plain file read at
/// startup rather than reconciled state — it changes when the platform's API
/// surface changes, which is a deployment.
///
/// # Errors
///
/// Returns a message if the file cannot be read or is not a valid catalogue.
pub(super) fn load(path: &Path) -> Result<ResourceCatalog, String> {
    let contents = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "could not read the resource catalogue from {}: {error}",
            path.display()
        )
    })?;

    serde_json::from_str(&contents).map_err(|error| {
        format!(
            "the resource catalogue at {} is malformed: {error}",
            path.display()
        )
    })
}
