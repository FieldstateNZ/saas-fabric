//! Persistence, concurrency and failed-write guarantees for local development.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use fabric_client_model::catalogue::Catalogue;
use fabric_control_plane::{ChangeContext, ClientRepository, RepositoryError};
use fabric_control_plane_api::local_repository::LocalClientRepository;

#[tokio::test]
async fn catalogue_survives_restart_and_conflicting_writes_are_refused() {
    let path = std::env::temp_dir().join(format!("fabric-local-test-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
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
    std::fs::remove_dir_all(path).unwrap();
}
