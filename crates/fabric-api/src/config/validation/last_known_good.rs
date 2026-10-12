//! Keeping the reader's last-known-good directory apart from what it reads.

use std::path::Path;

use fabric_tenant_runtime::check_separation;

use crate::config::AppConfig;

/// The publication record's name beside the tenants file, which the reader
/// also reads and the copy must stay apart from.
const RECORD_FILE: &str = "publication.json";

/// Refuses a last-known-good directory that is empty, or that is, holds or
/// sits inside the directory of any published file, with every symlink and
/// `..` resolved (`check_separation`, which the reader also runs before
/// every write).
///
/// The reader writes its copy there. Anywhere near the published files it
/// would be a second writer beside the publisher, which ADR 0018's single
/// writer rules out, and a published file that shares the copy's name would
/// be overwritten.
pub(super) fn validate(config: &AppConfig) -> Result<(), String> {
    let Some(directory) = &config.tenant_runtime.last_known_good_dir else {
        return Ok(());
    };
    let record = config.tenants_path.with_file_name(RECORD_FILE);
    let published: [&Path; 4] = [
        &config.tenants_path,
        &config.data_sources_path,
        &config.catalog_path,
        &record,
    ];
    check_separation(directory, &published)
        .map_err(|reason| format!("tenant_runtime.last_known_good_dir: {reason}"))
}
