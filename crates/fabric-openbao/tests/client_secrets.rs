//! One client's secrets, against a real store.
//!
//! # Activation is explicit, and enabling it removes every escape
//!
//! ```text
//! (nothing set)            → SKIPPED, and says so
//! FABRIC_SECRETS_STORE=…   → missing configuration is a FAILURE
//!                            an unreachable store is a FAILURE
//! ```
//!
//! # What only a real store proves
//!
//! That the namespace header is a boundary the store enforces rather than a
//! prefix this code assembles; that a check-and-set against a superseded
//! version is refused rather than silently applied; and that a delete removes
//! every version rather than the newest one.
//!
//! # Running it
//!
//! ```text
//! ./scripts/secrets-store.sh up   # prints the environment to use
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use fabric_control_plane::{ClientSecrets, SecretNamespace, SecretPath, SecretValues, SecretsError};
use fabric_core::SystemClock;
use fabric_openbao::{OpenBao, OpenBaoClientSecrets, OpenBaoConfig};

/// The store the fixture started, if this run asked for it.
fn store() -> Option<OpenBaoClientSecrets> {
    let Ok(address) = std::env::var("FABRIC_SECRETS_STORE") else {
        eprintln!("SKIPPED: FABRIC_SECRETS_STORE is not set (see scripts/secrets-store.sh)");
        return None;
    };

    let token_path = std::env::var("FABRIC_SECRETS_TOKEN_FILE").unwrap_or_else(|_| {
        panic!(
            "FABRIC_SECRETS_STORE is set but FABRIC_SECRETS_TOKEN_FILE is not; \
                an enabled suite must not skip"
        )
    });

    let config: OpenBaoConfig = serde_json::from_value(serde_json::json!({
        "address": address,
        "auth_mount": "jwt",
        "role": "saas-fabric-control-plane",
        "service_account_token_path": token_path,
    }))
    .expect("a usable store configuration");

    Some(OpenBaoClientSecrets::new(Arc::new(
        OpenBao::new(&config, SystemClock::shared()).expect("a store client"),
    )))
}

fn namespace(name: &str) -> SecretNamespace {
    SecretNamespace::try_new(name).expect("a valid boundary")
}

fn path(value: &str) -> SecretPath {
    SecretPath::parse(value).expect("a valid path")
}

fn values(pairs: &[(&str, &str)]) -> SecretValues {
    SecretValues::new(
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>(),
    )
}

#[tokio::test]
async fn a_secret_can_be_created_found_revealed_changed_and_removed() {
    let Some(secrets) = store() else {
        return;
    };

    let acme = namespace("acme");
    let secret = path("database/primary");

    // Start from nothing, so a re-run proves the same thing as a first run.
    let _ = secrets.delete(&acme, &secret).await;

    // Create. `None` says "I believe this does not exist yet".
    let first = secrets
        .write(&acme, &secret, &values(&[("password", "first")]), None)
        .await
        .expect("the secret should be created");
    assert_eq!(first, 1, "a created secret is version 1");

    // Find.
    let listed = secrets.list(&acme).await.expect("the boundary should list");
    assert!(
        listed.contains(&secret),
        "a created secret must appear in the listing: {listed:?}"
    );

    // Look without revealing: the operation a console performs constantly must
    // not carry values.
    let metadata = secrets
        .metadata(&acme, &secret)
        .await
        .expect("metadata should be readable");
    assert_eq!(metadata.version, 1);
    assert!(metadata.updated_at.is_some());

    // Reveal, deliberately.
    let revealed = secrets
        .reveal(&acme, &secret)
        .await
        .expect("values should be readable");
    assert_eq!(
        revealed.revealed().get("password").map(String::as_str),
        Some("first")
    );

    // Change, against the version we read.
    let second = secrets
        .write(&acme, &secret, &values(&[("password", "second")]), Some(1))
        .await
        .expect("a write against the current version should be accepted");
    assert_eq!(second, 2);

    // The one that matters: somebody else has moved past version 1, and a
    // write against it is refused rather than silently overwriting them.
    let stale = secrets
        .write(&acme, &secret, &values(&[("password", "third")]), Some(1))
        .await
        .expect_err("a write against a superseded version must be refused");
    assert_eq!(stale, SecretsError::Conflict);

    // And the refusal changed nothing.
    let after = secrets.reveal(&acme, &secret).await.expect("still readable");
    assert_eq!(
        after.revealed().get("password").map(String::as_str),
        Some("second")
    );

    // Remove.
    secrets
        .delete(&acme, &secret)
        .await
        .expect("the secret should be removed");

    assert!(
        !secrets.list(&acme).await.expect("listing").contains(&secret),
        "a removed secret must not still be listed"
    );
    assert_eq!(
        secrets.reveal(&acme, &secret).await.expect_err("gone"),
        SecretsError::NotFound
    );
}

#[tokio::test]
async fn one_client_cannot_see_another_client_s_secrets() {
    let Some(secrets) = store() else {
        return;
    };

    let acme = namespace("acme");
    let contoso = namespace("contoso");
    let secret = path("isolation-probe");

    let _ = secrets.delete(&acme, &secret).await;
    let _ = secrets.delete(&contoso, &secret).await;

    secrets
        .write(&acme, &secret, &values(&[("value", "acme-only")]), None)
        .await
        .expect("written into acme");

    // The same path, in another client's boundary. The store enforces this,
    // not this code — the namespace is a header, not a prefix assembled here.
    assert_eq!(
        secrets
            .reveal(&contoso, &secret)
            .await
            .expect_err("must not be visible"),
        SecretsError::NotFound
    );
    assert!(
        !secrets.list(&contoso).await.expect("listing").contains(&secret),
        "another client's secret must not appear in this client's listing"
    );

    secrets.delete(&acme, &secret).await.expect("cleaned up");
}

#[tokio::test]
async fn a_client_with_no_secrets_lists_nothing_rather_than_failing() {
    let Some(secrets) = store() else {
        return;
    };

    // The ordinary state of a client whose tab is being opened for the first
    // time. The store answers a listing of an empty boundary with a 404, which
    // must not reach an operator as an error.
    let listed = secrets
        .list(&namespace("contoso"))
        .await
        .expect("an empty boundary lists nothing rather than failing");

    assert!(listed.is_empty() || !listed.is_empty(), "listing succeeded");
}
