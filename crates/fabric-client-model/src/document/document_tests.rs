//! Tests for reading and editing a stored client document.

use crate::{ClientDocument, DesiredStateError, IdentityConfiguration, RealmName, RoleName};
use crate::{RedirectStrategyKind, RedirectUriKind};

/// A `v1` document with a section this model does not model, so both
/// preservation and the migrator are asserted rather than assumed.
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

/// The same client, written in the shape this model now writes.
const ACME_V2: &str = r"
apiVersion: fabric.fieldstate.nz/v2
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
    clients:
      - id: web
        type: oidc
        pkce: s256
        redirect:
          strategy: claimedHttps
          uris:
            - https://www.example.com/callback
";

fn acme() -> ClientDocument {
    ClientDocument::parse(ACME).unwrap()
}

/// A `v1` document whose one client declares the given callbacks.
fn v1_client_with(uris: &[&str]) -> String {
    let mut listed = String::new();
    for uri in uris {
        listed.push_str("          - ");
        listed.push_str(uri);
        listed.push('\n');
    }

    ACME.replace(
        "        redirectUris:\n          - https://www.example.com/callback\n",
        &format!("        redirectUris:\n{listed}"),
    )
}

/// The strategy the migrator read a `v1` client's callbacks as.
fn migrated_strategy(uris: &[&str]) -> RedirectStrategyKind {
    ClientDocument::parse(&v1_client_with(uris))
        .unwrap_or_else(|error| panic!("{uris:?} must migrate: {error}"))
        .into_client()
        .identity
        .clients
        .swap_remove(0)
        .redirect
        .kind()
        .clone()
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
    // Re-pointed at `v3` when `v2` shipped, so the rule it pins — an unknown
    // version is refused, not read as the one this model understands —
    // survives with its meaning intact.
    let text = ACME.replace("fabric.fieldstate.nz/v1", "fabric.fieldstate.nz/v3");

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::UnknownDocumentKind { .. })
    ));
}

#[test]
fn a_v2_document_reads_as_what_it_declares() {
    let client = ClientDocument::parse(ACME_V2).unwrap().into_client();
    let web = &client.identity.clients[0];

    assert_eq!(web.pkce, crate::PkceMethod::S256);
    assert_eq!(web.redirect.kind(), &RedirectStrategyKind::ClaimedHttps);
    assert_eq!(web.redirect.uris().len(), 1);
}

#[test]
fn a_v2_client_must_state_its_pkce_method() {
    // No default, following `type: oidc`'s precedent rather than
    // contradicting it: a defaulted field is a meaning a document acquires
    // without saying it, and the whole point of this one is that it is said.
    let text = ACME_V2.replace("        pkce: s256\n", "");

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::Malformed { .. })
    ));
}

#[test]
fn a_plain_pkce_method_is_refused_by_the_document() {
    let text = ACME_V2.replace("pkce: s256", "pkce: plain");

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::Malformed { .. })
    ));
}

#[test]
fn a_v2_document_carrying_the_replaced_field_is_told_what_replaced_it() {
    // Checked beside the document kind, for the reason already recorded there:
    // serde's "unknown field" would send an operator looking for a field their
    // document was never supposed to have.
    let text = ACME_V2.replace(
        "        redirect:\n          strategy: claimedHttps\n          uris:\n",
        "        redirectUris:\n",
    );

    let error = ClientDocument::parse(&text).unwrap_err();

    assert!(matches!(error, DesiredStateError::Migration { .. }), "{error}");
    assert!(error.to_string().contains("redirect"), "{error}");
    assert!(error.to_string().contains("strategy"), "{error}");
}

#[test]
fn a_v1_document_keeps_parsing_with_the_field_it_was_written_with() {
    // The pre-check above applies to `v2` only. In a `v1` document
    // `redirectUris` is not a mistake, it is the schema.
    let client = acme().into_client();

    assert_eq!(client.identity.clients[0].pkce, crate::PkceMethod::S256);
    assert_eq!(client.identity.clients[0].redirect.uris().len(), 1);
}

#[test]
fn a_v1_client_with_only_public_callbacks_reads_as_claimed_https() {
    assert_eq!(
        migrated_strategy(&["https://www.example.com/cb", "https://api.example.com/cb"]),
        RedirectStrategyKind::ClaimedHttps
    );
}

#[test]
fn a_v1_client_with_only_internal_callbacks_reads_as_private_network() {
    assert_eq!(
        migrated_strategy(&["http://acme.lucentroot.internal/cb", "https://x.internal/cb"]),
        RedirectStrategyKind::PrivateNetwork
    );
}

#[test]
fn a_v1_client_with_only_loopback_callbacks_reads_as_development() {
    assert_eq!(
        migrated_strategy(&["http://localhost:5173/cb", "http://127.0.0.1:8080/cb"]),
        RedirectStrategyKind::Development
    );
}

#[test]
fn a_v1_client_mixing_callback_kinds_must_be_migrated_by_hand() {
    // Refused rather than resolved: a client holding both a production
    // callback and a loopback one is the exact ambiguity the strategy exists
    // to remove, and picking the looser one would silently grant an
    // entitlement the operator never stated.
    let text = v1_client_with(&["https://www.example.com/cb", "http://localhost:5173/cb"]);
    let error = ClientDocument::parse(&text).unwrap_err();

    assert!(matches!(error, DesiredStateError::Migration { .. }), "{error}");
    assert!(error.to_string().contains("redirect"), "{error}");
}

#[test]
fn a_v1_client_with_a_private_use_scheme_must_be_migrated_by_hand() {
    // Cannot arise from a document `v1` could hold, because those schemes did
    // not parse before this change. The arm exists so the migrator stays total
    // now that they do.
    let text = v1_client_with(&["nz.fieldstate.slipway:/cb"]);
    let error = ClientDocument::parse(&text).unwrap_err();

    assert!(matches!(error, DesiredStateError::Migration { .. }), "{error}");
    assert!(error.to_string().contains("customScheme"), "{error}");
}

#[test]
fn a_v1_client_carrying_a_redirect_block_is_refused_rather_than_read_two_ways() {
    let text = ACME.replace(
        "        redirectUris:\n",
        "        redirect:\n          strategy: claimedHttps\n        redirectUris:\n",
    );

    assert!(matches!(
        ClientDocument::parse(&text),
        Err(DesiredStateError::Migration { .. })
    ));
}

#[test]
fn a_migrated_callback_keeps_the_kind_it_was_written_as() {
    let client = acme().into_client();
    let web = &client.identity.clients[0];

    assert_eq!(web.redirect.uris()[0].kind(), RedirectUriKind::Https);
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
fn editing_a_v1_document_returns_a_v2_document() {
    // Mechanically forced, not a preference: the edit writes the `v2` client
    // shape and then re-parses what it rendered, and a `v2` identity block
    // under a `v1` apiVersion cannot survive that. So the edit migrates the
    // document or it fails, and failing would make `v1` documents read-only
    // for no reason anybody asked for.
    let updated = with_extra_role(&acme());
    let rendered = updated.render().unwrap();

    assert!(
        rendered.contains("apiVersion: fabric.fieldstate.nz/v2"),
        "{rendered}"
    );
    assert!(!rendered.contains("redirectUris"), "{rendered}");
    assert!(rendered.contains("pkce: s256"), "{rendered}");
    assert!(rendered.contains("strategy: claimedHttps"), "{rendered}");

    // And it reads back as itself, which is what the re-parse guarantees.
    assert_eq!(
        ClientDocument::parse(&rendered).unwrap().client().identity,
        updated.client().identity
    );
}

#[test]
fn the_migrated_version_keeps_its_position_in_the_file() {
    // An ordered mapping's `insert` replaces in place, so the diff an operator
    // reviews is a one-line version change rather than a moved key.
    let rendered = with_extra_role(&acme()).render().unwrap();
    let position = |key: &str| rendered.find(key).unwrap();

    assert!(position("apiVersion") < position("kind:"));
    assert!(position("kind:") < position("metadata"));
    assert!(position("metadata") < position("spec"));
}

#[test]
fn a_v1_document_nobody_edits_stays_v1() {
    // Nothing reinterprets a document at rest. The migration happens on the
    // way into the typed view and never reaches the stored text.
    assert!(acme()
        .render()
        .unwrap()
        .contains("apiVersion: fabric.fieldstate.nz/v1"));
    assert!(acme().render().unwrap().contains("redirectUris:"));
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
