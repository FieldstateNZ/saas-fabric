//! What is stored about a registry, and what is not.

use super::record::SecretId;
use super::RegistryRecord;

#[test]
fn a_minted_id_names_a_credential_beneath_the_registries_and_nowhere_else() {
    let id = SecretId::mint().unwrap();
    let name = id.credential();

    assert!(name.as_str().starts_with("integrations/registries/"));
    assert!(name.as_str().ends_with("/credential"));
    assert_eq!(
        name.as_str().len(),
        "integrations/registries//credential".len() + 16
    );
}

#[test]
fn two_minted_ids_differ() {
    assert_ne!(SecretId::mint().unwrap(), SecretId::mint().unwrap());
}

#[test]
fn a_record_naming_its_secret_anywhere_else_is_unreadable() {
    // A record somebody edited by hand must not be able to point a
    // credential read or delete at another secret.
    for id in [
        "../git/app-private-key",
        "ABCDEF0123456789",
        "0123",
        "0123456789abcdefg",
    ] {
        let text = serde_json::json!({
            "host": "ghcr.io",
            "kind": "ghcr",
            "endpoint": "https://ghcr.io",
            "secretId": id,
            "registeredBy": "brett",
            "registeredAt": 1,
        });
        assert!(serde_json::from_value::<RegistryRecord>(text).is_err(), "{id}");
    }
}

#[test]
fn a_record_with_a_field_this_code_does_not_know_is_unreadable() {
    let text = serde_json::json!({
        "host": "ghcr.io",
        "kind": "ghcr",
        "endpoint": "https://ghcr.io",
        "secretId": "0123456789abcdef",
        "registeredBy": "brett",
        "registeredAt": 1,
        "token": "ghp_should-never-be-here",
    });
    assert!(serde_json::from_value::<RegistryRecord>(text).is_err());
}
