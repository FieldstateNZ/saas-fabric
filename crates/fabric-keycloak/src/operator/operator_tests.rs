//! Which address the realm's key set is read from.

use std::time::Duration;

use crate::OperatorRealmKeys;

const INTERNAL: &str = "http://keycloak-http.identity.svc.cluster.local/realms/master";

#[test]
fn keys_are_read_from_the_address_this_process_can_reach() {
    // A pod that cannot resolve the public issuer fetches no keys and refuses
    // every operator, with a log that says only that no key set arrived.
    let realm = OperatorRealmKeys::new(INTERNAL, Duration::from_secs(10)).expect("the fixture must build");

    assert_eq!(
        realm.jwks_endpoint,
        "http://keycloak-http.identity.svc.cluster.local/realms/master/protocol/openid-connect/certs"
    );
}

#[test]
fn a_trailing_slash_does_not_produce_a_doubled_path() {
    let realm = OperatorRealmKeys::new(&format!("{INTERNAL}/"), Duration::from_secs(10))
        .expect("the fixture must build");

    assert!(!realm.jwks_endpoint.contains("//protocol"));
}
