//! D01-15: a client on `.internal` hosts declares the scheme its callbacks
//! use, nothing infers one, and a document written before the field existed
//! is held rather than reinterpreted.

use crate::catalogue::{ClientProduct, ClientProductRequest};
use crate::{
    ClientDocument, ClientId, DesiredStateError, Host, PrivateNetworkScheme, ProjectionCheck,
    RedirectStrategyKind, RedirectUri,
};

const FIXTURE: &str = include_str!("../../../tests/fixtures/client.yaml");
const PUBLIC_HOSTS: &str = "  hosts:\n  - acme.example.com\n";
const PUBLIC_REDIRECT: &str = "        strategy: claimedHttps\n        uris:\n        - https://acme.reports.example.com/callback\n        - https://acme.example.com/reports/callback\n";
const INTERNAL_HOSTS: &str = "  hosts:\n  - acme.lucentroot.internal\n";

fn internal_redirect(scheme: &str) -> String {
    format!(
        "        strategy: privateNetwork\n        uris:\n        - {scheme}://acme.lucentroot.internal/reports/callback\n"
    )
}

fn document(hosts: &str, redirect: &str) -> ClientDocument {
    ClientDocument::parse(
        &FIXTURE
            .replace(PUBLIC_HOSTS, hosts)
            .replace(PUBLIC_REDIRECT, redirect),
    )
    .expect("the edited text must still parse")
}

fn callbacks(document: &ClientDocument) -> (RedirectStrategyKind, Vec<String>) {
    let projected = document.projected_clients().unwrap();
    let client = &projected[0];
    (
        client.redirect.kind().clone(),
        client
            .redirect
            .uris()
            .iter()
            .map(RedirectUri::to_string)
            .collect(),
    )
}

fn scheme_refusal(result: Result<impl std::fmt::Debug, DesiredStateError>) -> String {
    match result {
        Err(DesiredStateError::InvalidField {
            field: "privateNetworkScheme",
            detail,
        }) => detail,
        other => panic!("expected a privateNetworkScheme refusal, got {other:?}"),
    }
}

#[test]
fn each_declared_scheme_is_projected_as_declared() {
    for (scheme, written) in [("http", "http"), ("https", "https")] {
        let document = document(
            &format!("{INTERNAL_HOSTS}  privateNetworkScheme: {scheme}\n"),
            &internal_redirect(written),
        );

        assert_eq!(
            callbacks(&document),
            (
                RedirectStrategyKind::PrivateNetwork,
                vec![format!("{written}://acme.lucentroot.internal/reports/callback")]
            )
        );
        assert_eq!(document.projection_check(), ProjectionCheck::Consistent);
    }
}

#[test]
fn an_internal_client_with_no_declared_scheme_is_refused_not_inferred() {
    let document = document(INTERNAL_HOSTS, &internal_redirect("http"));

    let detail = scheme_refusal(document.projected_clients());
    assert!(detail.contains("http or https"), "{detail}");
}

#[test]
fn a_scheme_on_a_public_client_is_refused() {
    let document = document(
        &format!("{PUBLIC_HOSTS}  privateNetworkScheme: https\n"),
        PUBLIC_REDIRECT,
    );

    let detail = scheme_refusal(document.projected_clients());
    assert!(
        detail.contains("only to a client whose hosts are all .internal"),
        "{detail}"
    );
}

#[test]
fn an_unknown_scheme_does_not_parse() {
    for spelling in ["ftp", "HTTP", "''"] {
        let text = FIXTURE.replace(
            PUBLIC_HOSTS,
            &format!("{INTERNAL_HOSTS}  privateNetworkScheme: {spelling}\n"),
        );

        assert!(ClientDocument::parse(&text).is_err(), "{spelling}");
    }
}

#[test]
fn a_document_written_before_the_scheme_is_unverifiable_and_withholds_its_projection() {
    let document = document(INTERNAL_HOSTS, &internal_redirect("http"));
    let check = document.projection_check();

    assert!(matches!(check, ProjectionCheck::Unverifiable { .. }), "{check:?}");
    assert_eq!(check.withheld().len(), 1, "never read as either scheme");
}

#[test]
fn a_legacy_document_is_otherwise_consistent_only_when_its_entries_match_a_scheme() {
    assert!(document(INTERNAL_HOSTS, &internal_redirect("http")).undeclared_scheme_is_otherwise_consistent());
    assert!(document(INTERNAL_HOSTS, &internal_redirect("https")).undeclared_scheme_is_otherwise_consistent());

    let edited = internal_redirect("http").replace("/reports/callback", "/evil/callback");
    assert!(!document(INTERNAL_HOSTS, &edited).undeclared_scheme_is_otherwise_consistent());
    assert!(!ClientDocument::parse(FIXTURE)
        .unwrap()
        .undeclared_scheme_is_otherwise_consistent());
    assert!(!document(
        &format!("{INTERNAL_HOSTS}  privateNetworkScheme: http\n"),
        &internal_redirect("http")
    )
    .undeclared_scheme_is_otherwise_consistent());
}

fn request(hosts: &[&str], scheme: Option<PrivateNetworkScheme>) -> ClientProductRequest {
    ClientProductRequest {
        display_name: "Acme".into(),
        hosts: hosts.iter().map(|host| Host::try_new(host).unwrap()).collect(),
        private_network_scheme: scheme,
        legal_name: "Acme Limited".into(),
        region: "au".into(),
        timezone: "Pacific/Auckland".into(),
        configuration: std::collections::BTreeMap::new(),
        applications: Vec::new(),
    }
}

#[test]
fn a_product_write_records_the_declared_scheme_and_clears_it_when_hosts_go_public() {
    let stored = ClientDocument::parse(FIXTURE).unwrap();
    let product = stored.product().unwrap();

    let internal = stored
        .with_product(
            &request(&["acme.lucentroot.internal"], Some(PrivateNetworkScheme::Https)),
            &product,
        )
        .unwrap();
    assert_eq!(
        internal.client().private_network_scheme,
        Some(PrivateNetworkScheme::Https)
    );
    assert_eq!(
        callbacks(&internal).1,
        ["https://acme.lucentroot.internal/reports/callback"]
    );

    let public = internal
        .with_product(&request(&["acme.example.com"], None), &product)
        .unwrap();
    assert_eq!(public.client().private_network_scheme, None);
    assert!(!public.render().unwrap().contains("privateNetworkScheme"));
}

#[test]
fn a_product_write_refuses_a_missing_or_misplaced_scheme() {
    let stored = ClientDocument::parse(FIXTURE).unwrap();
    let product = stored.product().unwrap();

    scheme_refusal(stored.with_product(&request(&["acme.lucentroot.internal"], None), &product));
    scheme_refusal(stored.with_product(
        &request(&["acme.example.com"], Some(PrivateNetworkScheme::Http)),
        &product,
    ));
    scheme_refusal(ClientDocument::create(
        &ClientId::try_new("newco").unwrap(),
        &request(&["newco.example.com"], Some(PrivateNetworkScheme::Http)),
        &ClientProduct::default(),
    ));
}
