//! Persistence, concurrency and failed-write guarantees for local development.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fabric_client_model::catalogue::Catalogue;
use fabric_control_plane::{ChangeContext, ClientRepository, RepositoryError};
use fabric_control_plane_api::local_repository::LocalClientRepository;

/// A directory under the OS temp root, removed on drop no matter how the test
/// that owns it ends.
///
/// The version of this test this replaces named its directory by process id
/// alone and removed it only after every assertion passed, so a failing
/// assertion left the directory behind for the next run to trip over — and a
/// pid is not even unique across runs, only at a point in time. Pairing the
/// pid with a high-resolution timestamp makes the name unique per run rather
/// than per process, and tying removal to this guard's `Drop` — which fires
/// while a panicking `assert!` or `unwrap()` unwinds, not only on a clean
/// return — means a failure leaves nothing behind to explain to the next
/// person who runs this test.
struct TempDir(PathBuf);

impl TempDir {
    /// Creates a fresh directory named after `label`, the current process,
    /// and the current time.
    fn unique(label: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("the system clock must read after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{label}-{}-{stamp}", std::process::id()));

        std::fs::create_dir_all(&path).expect("the temp directory must be creatable");

        Self(path)
    }
}

impl std::ops::Deref for TempDir {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Best-effort: this runs during a panicking unwind as often as on a
        // clean return, and a second failure while cleaning up after the
        // first must not mask it.
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn catalogue_survives_restart_and_conflicting_writes_are_refused() {
    let path = TempDir::unique("fabric-local-test");
    let repository = LocalClientRepository::open(&path).await.unwrap();
    assert!(LocalClientRepository::open(&path).await.is_err());
    let context = ChangeContext {
        requested_by: "test".into(),
        summary: "settings".into(),
    };
    let mut catalogue = Catalogue::default();
    catalogue.settings.platform_name = "Persisted platform".into();
    let revision = repository
        .save_catalogue(&catalogue, None, &context)
        .await
        .unwrap();
    assert!(matches!(
        repository.save_catalogue(&catalogue, None, &context).await,
        Err(RepositoryError::Conflict)
    ));
    drop(repository);
    let repository = LocalClientRepository::open(&path).await.unwrap();
    let stored = repository.catalogue().await.unwrap();
    assert_eq!(stored.revision, Some(revision.clone()));
    assert_eq!(stored.catalogue.settings.platform_name, "Persisted platform");
    // A staging-path failure must leave both memory and the previous disk snapshot intact.
    std::fs::create_dir(path.join(".fabric-state.next")).unwrap();
    catalogue.settings.platform_name = "Must not commit".into();
    assert!(repository
        .save_catalogue(&catalogue, Some(&revision), &context)
        .await
        .is_err());
    assert_eq!(
        repository
            .catalogue()
            .await
            .unwrap()
            .catalogue
            .settings
            .platform_name,
        "Persisted platform"
    );
    drop(repository);
    assert_eq!(
        LocalClientRepository::open(&path)
            .await
            .unwrap()
            .catalogue()
            .await
            .unwrap()
            .catalogue
            .settings
            .platform_name,
        "Persisted platform"
    );
}
