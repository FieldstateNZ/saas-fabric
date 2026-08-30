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

/// A document written the way a human writes one: comments, blank lines,
/// quoting, a flow sequence, a folded scalar.
const HAND_WRITTEN: &str = r#"# Acme's client definition.
apiVersion: fabric.fieldstate.nz/v1
kind: Client
metadata:
  name: acme
spec:
  displayName: "Acme"      # quoted on purpose

  hosts: [www.example.com, api.example.com]

  identity:
    realm: acme
    roles:
      - Client Realm Administrator
      - Client Realm User
    clients: []

  features:
    invoicing: true
"#;

/// The fixture's identity with one more role, so an edit has something to do.
fn with_extra_role(document: &ClientDocument) -> ClientDocument {
    let mut identity = document.client().identity.clone();
    identity
        .roles
        .push(RoleName::try_new("Invoicing Approver").unwrap());

    document.with_identity(identity).unwrap()
}

#[test]
fn an_edit_preserves_every_other_key_and_value() {
    let document = ClientDocument::parse(HAND_WRITTEN).unwrap();

    let rendered = with_extra_role(&document).render().unwrap();
    let reread = ClientDocument::parse(&rendered).unwrap();

    // Values, including the ones this model does not understand.
    assert!(rendered.contains("invoicing: true"));
    assert_eq!(reread.client().display_name, "Acme");
    assert_eq!(reread.client().hosts.len(), 2);
    assert_eq!(reread.client().hosts[0].as_str(), "www.example.com");
    assert_eq!(reread.client().hosts[1].as_str(), "api.example.com");
}

#[test]
fn an_edit_preserves_the_order_keys_were_written_in() {
    // Part of the guarantee the documentation now states. A mapping that
    // reordered on write would turn every one-line change into a whole-file
    // diff, which is the thing that makes a Git-backed authority reviewable.
    let document = ClientDocument::parse(HAND_WRITTEN).unwrap();

    let rendered = with_extra_role(&document).render().unwrap();
    let position = |key: &str| rendered.find(key).unwrap();

    assert!(position("apiVersion") < position("kind"));
    assert!(position("kind") < position("metadata"));
    assert!(position("displayName") < position("hosts"));
    assert!(position("hosts:") < position("identity:"));
    assert!(position("identity:") < position("features:"));
}

#[test]
fn an_edit_does_not_preserve_formatting() {
    // Pinning a **limitation**, not a feature, because the documentation makes
    // a specific claim about it and prose nobody checks is prose that drifts —
    // this test exists because it did drift, and said "byte for byte".
    //
    // If a future parser starts preserving any of these, this test fails and
    // the documentation gets corrected in the same change rather than years
    // later.
    let document = ClientDocument::parse(HAND_WRITTEN).unwrap();

    let rendered = with_extra_role(&document).render().unwrap();

    assert!(
        !rendered.contains("# Acme's client definition."),
        "comments survived"
    );
    assert!(!rendered.contains("# quoted on purpose"), "comments survived");
    assert!(!rendered.contains("\n\n"), "blank lines survived");
    assert!(!rendered.contains("\"Acme\""), "quoting survived");
    assert!(
        !rendered.contains("[www.example.com"),
        "the flow sequence survived"
    );

    // What the flow sequence became, so the test says what *does* happen
    // rather than only what does not.
    assert!(rendered.contains("- www.example.com"));
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

/// A document that declares authorization, so the new section can be read
/// end to end rather than only unit-tested on its own type.
const WITH_AUTHORIZATION: &str = r"
apiVersion: fabric.fieldstate.nz/v1
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
  authorization:
    resources:
      - resource: customers
        relations:
          - name: viewer
            permits: [read, list]
          - name: owner
            permits: [read, list, create, update, delete]
";

#[test]
fn reads_a_declared_authorization_section() {
    let client = ClientDocument::parse(WITH_AUTHORIZATION).unwrap().into_client();

    let customers = &client.authorization.resources[0];
    assert_eq!(customers.resource.as_str(), "customers");
    assert_eq!(customers.relations.len(), 2);
    assert!(customers.relations[0].permits(fabric_core::OperationKind::List));
    assert!(!customers.relations[0].permits(fabric_core::OperationKind::Delete));
    assert!(customers.relations[1].permits(fabric_core::OperationKind::Delete));
}

#[test]
fn a_document_with_no_authorization_section_still_reads() {
    // Every document written before the section existed. Absent must mean
    // "not managed here", not "unreadable".
    let client = acme().into_client();

    assert!(client.authorization.is_empty());
}

#[test]
fn refuses_a_document_whose_authorization_is_invalid() {
    let broken = WITH_AUTHORIZATION.replace("permits: [read, list]", "permits: []");

    assert!(matches!(
        ClientDocument::parse(&broken),
        Err(DesiredStateError::InvalidField { .. })
    ));
}
