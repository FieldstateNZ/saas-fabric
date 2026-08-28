//! Tests for reading and editing a stored client document.

use crate::{ClientDocument, DesiredStateError, IdentityConfiguration, RealmName, RoleName};

/// A document with a section this model does not model, so preservation can be
/// asserted rather than assumed.
const ACME: &str = r"
apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: acme
spec:
  displayName: Acme
  hosts:
    - www.example.com
  identity:
    realm: acme
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients:
      - id: web
        type: oidc
        redirectUris:
          - https://www.example.com/callback
  features:
    invoicing: true
    analytics: false
  data:
    primary:
      class: dedicated
";

fn acme() -> ClientDocument {
    ClientDocument::parse(ACME).unwrap()
}

#[test]
fn reads_the_modelled_view_of_a_valid_document() {
    let client = acme().into_client();

    assert_eq!(client.id.as_str(), "acme");
    assert_eq!(client.display_name, "Acme");
    assert_eq!(client.hosts.len(), 1);
    assert_eq!(client.identity.realm.as_str(), "acme");
    assert_eq!(client.identity.roles.len(), 2);
    assert_eq!(client.identity.clients.len(), 1);
}

#[test]
fn a_document_of_another_kind_is_refused_as_the_wrong_kind() {
    let text = ACME.replace("kind: Client", "kind: Tenant");

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::UnknownDocumentKind { .. })
    ));
}

#[test]
fn a_future_api_version_is_refused_rather_than_read_as_this_one() {
    let text = ACME.replace("fabric.fieldstate.nz/v1", "fabric.fieldstate.nz/v2");

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::UnknownDocumentKind { .. })
    ));
}

#[test]
fn an_invalid_realm_is_refused() {
    let text = ACME.replace("realm: acme", "realm: Acme Corp");

    assert!(ClientDocument::parse(&text).is_err());
}

#[test]
fn a_document_missing_a_required_role_is_refused_on_read() {
    // The repository is not trusted to hold only valid documents: whatever
    // wrote it may not have been this code.
    let text = ACME.replace("      - Client Realm User\n", "");

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::RequiredRoleMissing { .. })
    ));
}

#[test]
fn an_empty_display_name_is_refused() {
    let text = ACME.replace("displayName: Acme", "displayName: \"  \"");

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::MissingField {
            field: "spec.displayName"
        })
    ));
}

#[test]
fn text_that_is_not_yaml_is_refused() {
    assert!(matches!(
        ClientDocument::parse("\tnot: [valid"),
        Err(DesiredStateError::Malformed { .. })
    ));
}

#[test]
fn editing_identity_preserves_every_other_section() {
    // The regression this exists for: a round trip through the modelled view
    // would delete `features` and `data`, and the only evidence would be a Git
    // diff nobody reads until invoicing stops working.
    let updated = acme()
        .with_identity(IdentityConfiguration {
            roles: vec![
                RoleName::try_new("Client Realm Administrator").unwrap(),
                RoleName::try_new("Client Realm User").unwrap(),
                RoleName::try_new("Invoicing Approver").unwrap(),
            ],
            ..acme().into_client().identity
        })
        .unwrap();

    let rendered = updated.render().unwrap();

    assert!(rendered.contains("invoicing: true"));
    assert!(rendered.contains("analytics: false"));
    assert!(rendered.contains("class: dedicated"));
    assert!(rendered.contains("Invoicing Approver"));
    assert!(rendered.contains("displayName: Acme"));
}

#[test]
fn an_edit_that_breaks_a_rule_produces_no_document() {
    let identity = IdentityConfiguration {
        roles: vec![RoleName::try_new("Client Realm User").unwrap()],
        ..acme().into_client().identity
    };

    assert!(matches!(
        acme().with_identity(identity),
        Err(DesiredStateError::RequiredRoleMissing {
            role: "Client Realm Administrator"
        })
    ));
}

#[test]
fn an_edited_document_reads_back_as_what_was_written() {
    let identity = IdentityConfiguration {
        realm: RealmName::try_new("acme").unwrap(),
        roles: vec![
            RoleName::try_new("Client Realm Administrator").unwrap(),
            RoleName::try_new("Client Realm User").unwrap(),
        ],
        clients: Vec::new(),
    };

    let updated = acme().with_identity(identity.clone()).unwrap();
    let reread = ClientDocument::parse(&updated.render().unwrap()).unwrap();

    assert_eq!(reread.client().identity, identity);
}
