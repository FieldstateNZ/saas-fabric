//! Keeping the reader's last-known-good directory apart from what it reads.

use std::path::Path;

use crate::config::AppConfig;

/// Refuses a last-known-good directory that is empty or holds any of the
/// published files.
///
/// The reader writes its copy there. In the published directory it would
/// be a second writer beside the publisher, which ADR 0018's single writer
/// rules out, and in Kubernetes that directory is a read-only `ConfigMap`
/// mount anyway, so the copy could never be written.
pub(super) fn validate(config: &AppConfig) -> Result<(), String> {
    let Some(directory) = &config.tenant_runtime.last_known_good_dir else {
        return Ok(());
    };
    if directory.as_os_str().is_empty() {
        return Err(
            "tenant_runtime.last_known_good_dir must not be empty; omit it to turn the copy off".to_owned(),
        );
    }
    let published = [
        &config.tenants_path,
        &config.data_sources_path,
        &config.catalog_path,
    ];
    if published
        .iter()
        .any(|path| path.parent() == Some(directory.as_path()) || *path == directory)
    {
        return Err(format!(
            "tenant_runtime.last_known_good_dir ({}) must not be the directory the published files are in: \
             the reader never writes where the publisher does",
            display(directory)
        ));
    }
    Ok(())
}

fn display(path: &Path) -> String {
    path.display().to_string()
}
