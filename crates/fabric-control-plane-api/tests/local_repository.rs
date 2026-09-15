//! Persistence, concurrency and failed-write guarantees for local development.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fabric_client_model::catalogue::Catalogue;
use fabric_client_model::{ClientDocument, ClientRevision};
use fabric_control_plane::{ChangeContext, ClientRepository, RepositoryError};
use fabric_control_plane_api::local_repository::LocalClientRepository;

/// A minimal, complete client document — enough for `ClientDocument::parse`
/// to accept it and for it to import as a top-level `*.yaml` file.
const ACME: &str = r"apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: acme
spec:
  displayName: Acme
  identity:
    realm: acme
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []
";

fn change() -> ChangeContext {
    ChangeContext {
        requested_by: "test".into(),
        summary: "test fixture".into(),
    }
}

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

#[tokio::test]
async fn a_client_create_and_update_are_each_compare_and_swap() {
    let path = TempDir::unique("fabric-local-client-cas");
    let repository = LocalClientRepository::open(&path).await.unwrap();
    let document = ClientDocument::parse(ACME).unwrap();
    let id = document.client().id.clone();

    let revision = repository.create(&document, &change()).await.unwrap();

    // A duplicate create is refused — the id is already a document, not a
    // revision that moved.
    assert!(matches!(
        repository.create(&document, &change()).await,
        Err(RepositoryError::Conflict)
    ));

    // A stale update is refused.
    let stale = ClientRevision::try_new("not-the-current-revision").unwrap();
    assert!(matches!(
        repository.update(&id, &document, &stale, &change()).await,
        Err(RepositoryError::Conflict)
    ));

    // Accepted at the revision `create` actually produced, and moves it.
    let moved = repository
        .update(&id, &document, &revision, &change())
        .await
        .unwrap();
    assert_ne!(moved, revision);
    assert_eq!(repository.get(&id).await.unwrap().revision, moved);
}

#[tokio::test]
async fn a_top_level_yaml_file_is_imported_on_first_open() {
    let path = TempDir::unique("fabric-local-import");
    std::fs::write(path.join("acme.yaml"), ACME).unwrap();

    let repository = LocalClientRepository::open(&path).await.unwrap();

    let stored = repository
        .get(&ClientDocument::parse(ACME).unwrap().client().id)
        .await
        .unwrap();
    assert_eq!(stored.document.client().display_name, "Acme");
}

#[tokio::test]
async fn opening_a_directory_with_an_unparseable_document_is_refused() {
    let path = TempDir::unique("fabric-local-bad-document");
    std::fs::write(path.join("broken.yaml"), "kind: Tenant\n").unwrap();

    assert!(LocalClientRepository::open(&path).await.is_err());
}

#[tokio::test]
async fn a_snapshot_predating_the_versioned_catalogue_names_the_state_file_and_is_refused() {
    let path = TempDir::unique("fabric-local-legacy-catalogue");
    std::fs::write(
        path.join(".fabric-state.json"),
        r#"{"writes":1,"clients":{},"catalogue":{"revision":"local-1","text":"applications: []\n"}}"#,
    )
    .unwrap();

    let message = match LocalClientRepository::open(&path).await {
        Ok(_) => panic!("a pre-envelope catalogue must be refused, not opened"),
        Err(error) => error.to_string(),
    };

    assert!(message.contains(".fabric-state.json"), "{message}");
    assert!(message.contains("predates the versioned catalogue"), "{message}");
}
