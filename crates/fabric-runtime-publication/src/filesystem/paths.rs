//! Where one document's payload and manifest live on disk.

use std::path::{Path, PathBuf};

use crate::DocumentKind;

/// The payload path a caller gave us, and the manifest path derived from it.
///
/// The manifest always sits beside the payload, under the crate's own fixed
/// file name for this [`DocumentKind`] ([`DocumentKind::manifest_file`]) —
/// never derived by rewriting whatever the caller happened to name the
/// payload file, so a caller who points `tenants_path` at something other
/// than `tenants.json` still gets a manifest under the name the rest of this
/// crate recognises.
pub(super) struct DocumentPaths {
    /// Which document this is, for every error and manifest this path pair
    /// produces.
    pub(super) kind: DocumentKind,
    /// The payload file's path, exactly as the caller gave it.
    pub(super) payload: PathBuf,
    /// The manifest file's path: the payload's parent directory, joined
    /// with `kind`'s own manifest file name.
    pub(super) manifest: PathBuf,
}

impl DocumentPaths {
    /// Builds one document's path pair. The manifest file name comes from
    /// `kind` itself ([`DocumentKind::manifest_file`]) — never a literal
    /// passed in, and never derived from `payload`.
    pub(super) fn new(kind: DocumentKind, payload: PathBuf) -> Self {
        let directory = payload.parent().unwrap_or_else(|| Path::new("."));
        let manifest = directory.join(kind.manifest_file());

        Self {
            kind,
            payload,
            manifest,
        }
    }
}
