//! What a token request asks for, whatever the realm's URL carried.

use crate::client::realm::Allowed;
use crate::client::scope::Scope;
use crate::client::token::token_url;

fn allowed(realm: &str, service: Option<&str>) -> Allowed {
    Allowed {
        realm: reqwest::Url::parse(realm).unwrap(),
        service: service.map(ToOwned::to_owned),
    }
}

#[test]
fn a_repository_is_asked_for_pull_alone() {
    let url = token_url(
        &allowed("https://ghcr.io/token", Some("ghcr.io")),
        Scope::Repository("fieldstatenz/saas-fabric"),
    );

    assert_eq!(
        url.as_str(),
        "https://ghcr.io/token?service=ghcr.io&scope=repository%3Afieldstatenz%2Fsaas-fabric%3Apull"
    );
}

#[test]
fn a_scope_or_service_the_realm_carried_is_dropped() {
    let realm = allowed(
        "https://auth.example.com/token?scope=repository:*:push&service=elsewhere&tenant=acme",
        Some("registry.example.com"),
    );

    let proving = token_url(&realm, Scope::Registry);
    let pairs: Vec<(String, String)> = proving.query_pairs().into_owned().collect();
    assert_eq!(
        pairs,
        vec![
            ("tenant".to_owned(), "acme".to_owned()),
            ("service".to_owned(), "registry.example.com".to_owned()),
        ],
        "proving asks no scope, whatever the realm offered"
    );

    let reading = token_url(&realm, Scope::Repository("team/app"));
    let scopes: Vec<String> = reading
        .query_pairs()
        .filter(|(name, _)| name == "scope")
        .map(|(_, value)| value.into_owned())
        .collect();
    assert_eq!(scopes, vec!["repository:team/app:pull".to_owned()]);
}

#[test]
fn a_realm_with_no_query_and_no_service_asks_only_the_scope() {
    let url = token_url(&allowed("https://auth.example.com/token", None), Scope::Registry);

    assert_eq!(url.as_str(), "https://auth.example.com/token");
}
