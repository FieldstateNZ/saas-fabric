//! Two integrations, and nothing either one touches belongs to the other.

use std::sync::Arc;

use crate::git_integration::{GitIntegration, InMemoryIntegrationStore, IntegrationKind, IntegrationStore};

const BOTH: [IntegrationKind; 2] = [
    IntegrationKind::ClientConfiguration,
    IntegrationKind::PlatformManagement,
];

#[test]
fn the_client_private_key_is_where_it_already_was() {
    // A connected instance keeps its key at this name. Moving it to make the
    // two look symmetrical would be a migration whose upside is tidiness and
    // whose downside is a platform that cannot reach client configuration.
    assert_eq!(
        IntegrationKind::ClientConfiguration.private_key(),
        "git/app-private-key"
    );
}

#[test]
fn no_two_integrations_share_a_secret_name() {
    let names: Vec<&str> = BOTH.iter().map(|kind| kind.private_key()).collect();

    assert_eq!(names.len(), 2);
    assert_ne!(
        names[0], names[1],
        "connecting one integration would overwrite the other's key"
    );
}

#[tokio::test]
async fn saving_one_integration_leaves_the_other_absent() {
    let store = InMemoryIntegrationStore::new();

    store
        .save(
            IntegrationKind::PlatformManagement,
            &GitIntegration::created("5678", "saas-fabric-platform"),
        )
        .await
        .unwrap();

    assert!(store
        .load(IntegrationKind::ClientConfiguration)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .load(IntegrationKind::PlatformManagement)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn clearing_one_integration_leaves_the_other_connected() {
    // Removal is where a shared key would be unrecoverable: reconnecting the
    // wrong one is an inconvenience, deleting the other one's record is a
    // platform that has silently stopped managing something.
    for removed in BOTH {
        let kept = if removed == IntegrationKind::ClientConfiguration {
            IntegrationKind::PlatformManagement
        } else {
            IntegrationKind::ClientConfiguration
        };

        let store = Arc::new(InMemoryIntegrationStore::new());
        for kind in BOTH {
            store
                .save(kind, &GitIntegration::created("1234", "saas-fabric"))
                .await
                .unwrap();
        }

        store.clear(removed).await.unwrap();

        assert!(
            store.load(removed).await.unwrap().is_none(),
            "{removed:?} was not removed"
        );
        assert!(
            store.load(kept).await.unwrap().is_some(),
            "removing {removed:?} took {kept:?} with it"
        );
    }
}

#[tokio::test]
async fn nothing_stored_for_a_kind_is_absence_and_not_a_failure() {
    let store = InMemoryIntegrationStore::new();

    for kind in BOTH {
        assert!(store.load(kind).await.expect("absence is not an error").is_none());
    }
}
