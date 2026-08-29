//! What the integration record says about itself.

use super::*;

fn installed_on(repository: Option<SelectedRepository>) -> GitIntegration {
    GitIntegration {
        installation: Some(Installation {
            id: "42".to_owned(),
            account: "FieldstateNZ".to_owned(),
            repository,
        }),
        ..GitIntegration::created("1", "saas-fabric")
    }
}

#[test]
fn a_created_app_is_not_yet_usable() {
    // Creating the App and installing it are separate approvals on GitHub, and
    // an operator can do the first and abandon the second.
    let integration = GitIntegration::created("1", "saas-fabric");

    assert!(integration.installation.is_none());
    assert!(!integration.is_usable());
    assert!(integration.repository().is_none());
}

#[test]
fn an_installation_with_no_repository_settled_is_not_usable_either() {
    // An installation granting access to several repositories is a real state,
    // and guessing which one holds client configuration would write somewhere
    // nobody expects and look like it worked.
    assert!(!installed_on(None).is_usable());
}

#[test]
fn an_installation_with_a_repository_is_usable() {
    let integration = installed_on(Some(SelectedRepository::conventional(
        "FieldstateNZ",
        "saas-fabric-clients",
        "main",
    )));

    assert!(integration.is_usable());
    assert_eq!(
        integration.repository().map(SelectedRepository::describe),
        Some("FieldstateNZ/saas-fabric-clients".to_owned())
    );
}

#[test]
fn the_document_layout_is_a_convention_rather_than_a_choice() {
    // Both sides of the layout belong to this platform: it writes these
    // documents and it reads them. Two deployments disagreeing about where a
    // client lives in the same repository is not a configuration option.
    let repository = SelectedRepository::conventional("owner", "repo", "main");

    assert_eq!(repository.path_prefix, "clients");
    assert_eq!(repository.document_file, "client.yaml");
}

#[test]
fn a_record_round_trips_through_its_serialised_form() {
    // It is written to a store and read back after a restart; a field that
    // does not survive that is a field the platform silently forgets.
    let integration = installed_on(Some(SelectedRepository::conventional(
        "FieldstateNZ",
        "saas-fabric-clients",
        "main",
    )));

    let encoded = serde_json::to_string(&integration).expect("the record must serialise");
    let decoded: GitIntegration = serde_json::from_str(&encoded).expect("and must read back");

    assert_eq!(decoded, integration);
}

#[test]
fn no_credential_is_carried_in_the_record() {
    // The private key belongs to the secret partition. This is the type whose
    // contents the API is allowed to describe to an operator, so a credential
    // reaching it would be a credential reaching a browser.
    let encoded = serde_json::to_string(&installed_on(Some(SelectedRepository::conventional(
        "o", "r", "main",
    ))))
    .expect("the record must serialise");

    for forbidden in ["key", "pem", "secret", "token", "password"] {
        assert!(
            !encoded.to_lowercase().contains(forbidden),
            "the record must not carry anything called {forbidden}: {encoded}"
        );
    }
}
