//! Which of a realm's two addresses each endpoint is built from.

use std::time::Duration;

use crate::RealmSignIn;

const ISSUER: &str = "https://auth.example.test/realms/master";
const INTERNAL: &str = "http://keycloak-http.identity.svc.cluster.local/realms/master";

fn sign_in(reachable_at: &str) -> RealmSignIn {
    RealmSignIn::new(
        ISSUER,
        reachable_at,
        "saas-fabric-console",
        "https://fabric.example.test/",
        Duration::from_secs(10),
    )
    .expect("the fixture must build")
}

#[test]
fn the_browser_is_sent_to_the_issuer_even_when_this_process_uses_another_address() {
    // The operator's browser resolves the public hostname; sending it to a
    // cluster-internal service address would be sending it nowhere.
    let realm = sign_in(INTERNAL);

    assert_eq!(
        realm.authorization_endpoint,
        "https://auth.example.test/realms/master/protocol/openid-connect/auth"
    );
}

#[test]
fn keys_and_redemption_use_the_address_this_process_can_reach() {
    // The reverse of the case above, and the one that fails silently: a
    // pod that cannot resolve the issuer fetches no keys and refuses every
    // operator, with a log that says only that no key set arrived.
    let realm = sign_in(INTERNAL);

    assert_eq!(
        realm.jwks_endpoint,
        "http://keycloak-http.identity.svc.cluster.local/realms/master/protocol/openid-connect/certs"
    );
    assert_eq!(
        realm.token_endpoint,
        "http://keycloak-http.identity.svc.cluster.local/realms/master/protocol/openid-connect/token"
    );
}

#[test]
fn one_url_serves_both_when_a_deployment_states_one() {
    let realm = sign_in(ISSUER);

    assert!(realm.jwks_endpoint.starts_with(ISSUER));
    assert!(realm.authorization_endpoint.starts_with(ISSUER));
}

#[test]
fn a_trailing_slash_on_either_does_not_produce_a_doubled_path() {
    let realm = RealmSignIn::new(
        &format!("{ISSUER}/"),
        &format!("{INTERNAL}/"),
        "saas-fabric-console",
        "https://fabric.example.test/",
        Duration::from_secs(10),
    )
    .expect("the fixture must build");

    assert!(!realm.jwks_endpoint.contains("//protocol"));
    assert!(!realm.authorization_endpoint.contains("//protocol"));
}
