//! Which files the restore may read and append to: never one that is, or
//! resolves to, a file the publisher holds, nor one another of them is.
//!
//! Paths are compared by what they open, not by how they are spelt: the
//! device and inode of every published file, followed through any symbolic
//! link, against those of the source and the audit log. A hard link or a
//! symbolic link to a published file, or an audit log that is the source,
//! is refused before anything is written.

use std::fs::{File, Metadata};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use fabric_runtime_publication::{DocumentKind, ISSUER_REGISTRY_FILE, PUBLICATION_RECORD_FILE};

/// A file's device and inode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Identity(u64, u64);

#[cfg(unix)]
pub(super) fn identity(metadata: &Metadata) -> Identity {
    use std::os::unix::fs::MetadataExt as _;
    Identity(metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
pub(super) fn identity(_metadata: &Metadata) -> Identity {
    Identity(0, 0)
}

/// Every file the filesystem publisher holds or reserves beside these
/// payloads: the payloads, their manifests, the record, the registry, the
/// lock, and the staging sibling each write goes through, which a restore
/// clears and replaces.
pub(super) fn published_files(tenants: &Path, data_sources: &Path, catalog: &Path) -> Vec<PathBuf> {
    let beside = |payload: &Path, name: &str| payload.parent().unwrap_or_else(|| Path::new(".")).join(name);
    let lock = format!(
        ".{}.lock",
        tenants
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default()
    );
    let held = [
        tenants.to_path_buf(),
        data_sources.to_path_buf(),
        catalog.to_path_buf(),
        beside(tenants, DocumentKind::Tenants.manifest_file()),
        beside(data_sources, DocumentKind::DataSources.manifest_file()),
        beside(catalog, DocumentKind::Catalog.manifest_file()),
        beside(tenants, PUBLICATION_RECORD_FILE),
        beside(tenants, ISSUER_REGISTRY_FILE),
    ];
    let staging = held.iter().map(|path| {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        beside(path, &format!(".{name}.tmp"))
    });
    let mut files: Vec<PathBuf> = staging.collect();
    files.extend(held);
    files.push(beside(tenants, &lock));
    files
}

/// The identities of every published file that exists.
pub(super) fn identities(paths: &[PathBuf]) -> Vec<Identity> {
    paths
        .iter()
        .filter_map(|path| std::fs::metadata(path).ok())
        .map(|metadata| identity(&metadata))
        .collect()
}

/// Reads the source through one handle, refusing one that is not a regular
/// file or is any published file. The handle is returned so the source can
/// be checked again, by what it is, inside the publication lock.
pub(super) fn read_source(path: &Path, published: &[Identity]) -> Result<(Vec<u8>, File), String> {
    let mut file = File::open(path).map_err(|error| format!("--from cannot be opened: {error}"))?;
    let metadata = file
        .metadata()
        .map_err(|error| format!("--from cannot be examined: {error}"))?;
    if !metadata.is_file() {
        return Err("--from is not a regular file".to_owned());
    }
    let source = identity(&metadata);
    if published.contains(&source) {
        return Err("--from is a published file; give a copy kept apart from the target".to_owned());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| format!("--from cannot be read: {error}"))?;
    Ok((bytes, file))
}
