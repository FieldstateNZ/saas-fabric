//! Each kind's realm rule, judged with no connection anywhere.

use fabric_platform_management::RegistryError;

use super::realm::Realm;
use crate::settings::RealmRule;

fn realm(rule: &RealmRule) -> Realm {
    Realm::from_rule(rule).unwrap()
}

fn refusal(result: Result<super::realm::Allowed, RegistryError>) -> String {
    match result {
        Err(RegistryError::Refused { detail }) => detail,
        Err(other) => panic!("expected Refused, got {other:?}"),
        Ok(allowed) => panic!("expected a refusal, allowed {}", allowed.realm),
    }
}

#[test]
fn a_fixed_realm_is_asked_with_its_own_service_whatever_the_challenge_said() {
    let docker_hub = realm(&RealmRule::Fixed {
        realm: "https://auth.docker.io/token".to_owned(),
        service: "registry.docker.io".to_owned(),
    });

    let allowed = docker_hub
        .allow("reading", "https://auth.docker.io/token", Some("elsewhere"))
        .unwrap();

    assert_eq!(allowed.realm.as_str(), "https://auth.docker.io/token");
    assert_eq!(allowed.service.as_deref(), Some("registry.docker.io"));
}

#[test]
fn a_fixed_realm_refuses_another_origin_and_names_both() {
    let ghcr = realm(&RealmRule::Fixed {
        realm: "https://ghcr.io/token".to_owned(),
        service: "ghcr.io".to_owned(),
    });

    let detail = refusal(ghcr.allow("reading", "https://evil.example/token?x=secret", None));

    assert!(detail.contains("https://evil.example"), "{detail}");
    assert!(detail.contains("https://ghcr.io"), "{detail}");
    assert!(
        !detail.contains("secret") && !detail.contains("/token"),
        "{detail}"
    );
}

#[test]
fn a_recorded_origin_allows_its_own_realm_and_refuses_a_changed_one() {
    let recorded = realm(&RealmRule::Recorded {
        origin: Some("https://auth.example.com".to_owned()),
    });

    let allowed = recorded
        .allow(
            "reading",
            "https://auth.example.com/jwt/auth",
            Some("container_registry"),
        )
        .unwrap();
    assert_eq!(allowed.realm.as_str(), "https://auth.example.com/jwt/auth");
    assert_eq!(allowed.service.as_deref(), Some("container_registry"));

    let detail = refusal(recorded.allow("reading", "https://auth.example.com:8443/jwt/auth", None));
    assert!(detail.contains("https://auth.example.com:8443"), "{detail}");
}

#[test]
fn a_registry_recorded_with_no_realm_refuses_one_named_later() {
    let recorded = realm(&RealmRule::Recorded { origin: None });

    let detail = refusal(recorded.allow("reading", "https://auth.example.com/token", None));

    assert!(detail.contains("none was recorded"), "{detail}");
}

#[test]
fn following_a_challenge_records_the_first_origin_and_refuses_another() {
    let deployment = realm(&RealmRule::FollowChallenge);

    deployment
        .allow("reading", "https://ghcr.io/token", None)
        .unwrap();
    deployment
        .allow("reading", "https://ghcr.io/token?again", None)
        .unwrap();
    let detail = refusal(deployment.allow("reading", "https://other.example/token", None));

    assert!(detail.contains("https://ghcr.io"), "{detail}");
    assert!(detail.contains("https://other.example"), "{detail}");
}

#[test]
fn a_realm_carrying_a_credential_or_not_a_url_is_refused() {
    let deployment = realm(&RealmRule::FollowChallenge);

    let detail = refusal(deployment.allow("reading", "https://user:pass@ghcr.io/token", None));
    assert!(!detail.contains("pass"), "{detail}");
    refusal(deployment.allow("reading", "not a url", None));
}
