//! A directory the publisher writes into, removed when the test is done.
//!
//! `tempfile` is not in this workspace's dependency table, so this is the
//! hand-rolled equivalent, the same shape as
//! `fabric-runtime-publication/tests/support/mod.rs::TempDir`. Deliberately
//! not shared with that crate: this crate must not depend on it at all (see
//! `src/lib.rs`), and duplicating ~30 lines is cheaper than the dependency
//! that would avoid it.

use std::path::PathBuf;

/// A fresh, empty directory under the system temp root, removed on drop.
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a fresh, empty directory.
    ///
    /// The name mixes a process id, a nanosecond timestamp, and a counter --
    /// the same three-part scheme `names::RunId` uses, for the same reason:
    /// several tests in this binary can call this within the same
    /// nanosecond, and two of them landing on one directory is worse than a
    /// slightly longer name.
    #[must_use]
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

        let unique = format!(
            "fabric-ndc-acceptance-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    /// Where a publisher would write the tenants payload.
    #[must_use]
    pub fn tenants_path(&self) -> PathBuf {
        self.path.join("tenants.json")
    }

    /// Where a publisher would write the data-sources payload.
    #[must_use]
    pub fn data_sources_path(&self) -> PathBuf {
        self.path.join("data-sources.json")
    }

    /// Where a publisher would write the catalogue payload.
    #[must_use]
    pub fn catalog_path(&self) -> PathBuf {
        self.path.join("catalog.json")
    }
}

impl Default for TempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
