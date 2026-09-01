//! What the platform asks the Git host to create.
//!
//! Every assertion here is about a decision that is invisible once the
//! application exists: permissions cannot be narrowed after the fact without
//! an operator re-approving, and a manifest that asked for a webhook would
//! have GitHub retrying deliveries at a host it can never reach.

use super::{manifest, AppPurpose};

const CALLBACK: &str = "https://fabric-lucentroot.tailnet.ts.net";

/// The purpose the first application was created with, which is now one of
/// two. Its name and segment are asserted below rather than described,
/// because deployments already have an application answering to both.
fn client_configuration() -> AppPurpose {
    AppPurpose {
        name: "SaaS Fabric".to_owned(),
        callback_segment: "git".to_owned(),
    }
}

fn manifest_value() -> serde_json::Value {
    manifest::build(CALLBACK, &client_configuration())
}

#[test]
fn asks_only_for_what_writing_client_documents_needs() {
    let permissions = &manifest_value()["default_permissions"];

    assert_eq!(permissions["contents"], "write");
    assert_eq!(permissions["metadata"], "read");
    assert_eq!(
        permissions.as_object().map(serde_json::Map::len),
        Some(2),
        "a permission nobody asked for is one an operator has to re-approve to remove"
    );
}

#[test]
fn does_not_ask_for_repository_administration() {
    // Workspec's equivalent does, because it creates repositories. Creating
    // them is a stated non-goal here, and an application that can create a
    // repository can also rename and transfer one.
    assert!(manifest_value()["default_permissions"]
        .get("administration")
        .is_none());
}

#[test]
fn subscribes_to_no_events_and_declares_no_webhook() {
    // The control plane is published on the operator plane and on no public
    // one. A hook would be a URL GitHub can never deliver to.
    let value = manifest_value();

    assert_eq!(value["default_events"].as_array().map(Vec::len), Some(0));
    assert!(value.get("hook_attributes").is_none());
}

#[test]
fn is_private_so_nobody_else_can_install_it() {
    assert_eq!(manifest_value()["public"], false);
}

#[test]
fn carries_the_host_in_its_name_because_the_name_is_globally_unique() {
    // Two SaaS Fabric deployments must both be able to create an application
    // without one of them failing on a name clash.
    let value = manifest_value();
    let name = value["name"].as_str().unwrap_or_default();

    assert!(name.contains("fabric-lucentroot.tailnet.ts.net"), "{name}");
    assert!(
        !name.contains("https://"),
        "the scheme is not part of a name: {name}"
    );
}

#[test]
fn the_client_configuration_application_is_named_and_routed_as_it_always_was() {
    // Deployments have already created this application, under this name, with
    // these two callbacks stored on the host. Purpose became an input so that a
    // second application could exist; it must not have moved the first one.
    let value = manifest_value();

    assert_eq!(value["name"], "SaaS Fabric — fabric-lucentroot.tailnet.ts.net");
    assert_eq!(
        value["redirect_url"],
        format!("{CALLBACK}/api/integrations/git/created")
    );
}

#[test]
fn a_second_purpose_asks_for_a_different_name_and_lands_somewhere_else() {
    // The name, because GitHub rejects the second application otherwise. The
    // callbacks, because a browser finishing one flow must not be handed to
    // the other flow's routes, which would connect the wrong integration.
    let platform = manifest::build(
        CALLBACK,
        &AppPurpose {
            name: "SaaS Fabric Platform".to_owned(),
            callback_segment: "platform".to_owned(),
        },
    );
    let client = manifest_value();

    assert_ne!(platform["name"], client["name"]);
    assert_eq!(
        platform["redirect_url"],
        format!("{CALLBACK}/api/integrations/platform/created")
    );
    assert_eq!(
        platform["setup_url"],
        format!("{CALLBACK}/api/integrations/platform/installed")
    );

    // Both write files to a repository an operator chose. Neither has a
    // narrower set of permissions to ask for than the other.
    assert_eq!(platform["default_permissions"], client["default_permissions"]);
}

#[test]
fn returns_the_browser_to_this_platform_after_creation_and_after_install() {
    let value = manifest_value();

    assert_eq!(
        value["redirect_url"],
        format!("{CALLBACK}/api/integrations/git/created")
    );
    assert_eq!(
        value["setup_url"],
        format!("{CALLBACK}/api/integrations/git/installed")
    );

    // An operator changing which repositories are shared must land back in the
    // console rather than on a GitHub page with nowhere to go.
    assert_eq!(value["setup_on_update"], true);
}

#[test]
fn the_creation_url_is_organisation_scoped() {
    // The application belongs to the organisation whose client configuration
    // it reads, not to whoever happened to set it up — one owned by a personal
    // account leaves when that person does.
    assert_eq!(
        manifest::creation_url("https://github.com", "FieldstateNZ", "the-state"),
        "https://github.com/organizations/FieldstateNZ/settings/apps/new?state=the-state"
    );
}

#[test]
fn interpolated_values_are_encoded_rather_than_trusted() {
    let url = manifest::install_url("https://github.com", "slug/../../evil", "a b&c=d");

    assert!(
        !url.contains("../"),
        "a slug from the host must not be able to reach another path: {url}"
    );
    assert!(url.contains("a%20b%26c%3Dd"), "{url}");
}

#[test]
fn the_install_url_carries_the_state_the_flow_started_with() {
    let url = manifest::install_url("https://github.com", "saas-fabric", "the-state");

    assert_eq!(
        url,
        "https://github.com/apps/saas-fabric/installations/new?state=the-state"
    );
}
