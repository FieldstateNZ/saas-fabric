use fabric_core::TenantId;

use super::*;

fn registration(issuer: &str, tenant: &str) -> IssuerRegistration {
    IssuerRegistration {
        issuer: RegistryIssuer::parse(issuer).unwrap(),
        tenant: TenantId::try_new(tenant).unwrap(),
    }
}

#[test]
fn an_issuer_is_accepted_only_in_the_platform_contracts_shape() {
    for accepted in [
        "https://identity.example.invalid/realms/alpha",
        "https://id.example.invalid:8443/realms/Alpha_1-b",
        "https://10.0.0.1/realms/a",
    ] {
        assert!(RegistryIssuer::parse(accepted).is_ok(), "{accepted}");
    }
    for refused in [
        "http://identity.example.invalid/realms/alpha",
        "https://identity.example.invalid/realms/",
        "https://identity.example.invalid/auth/realms/alpha",
        "https://Identity.example.invalid/realms/alpha",
        "https://identity.example.invalid/realms/alpha/",
        "https://identity.example.invalid/realms/alpha?x=1",
        "https://identity.example.invalid:/realms/alpha",
        "https://identity.example.invalid:123456/realms/alpha",
        "https:///realms/alpha",
        "https://identity.example.invalid/realms/a/realms/b",
    ] {
        assert!(RegistryIssuer::parse(refused).is_err(), "{refused}");
    }
}

#[test]
fn an_origin_names_each_realms_issuer_and_refuses_a_path() {
    let origin = IssuerOrigin::parse("https://identity.example.invalid").unwrap();

    assert_eq!(
        origin.issuer("acme").unwrap().as_str(),
        "https://identity.example.invalid/realms/acme"
    );
    assert!(origin.issuer("ac/me").is_err());
    assert!(IssuerOrigin::parse("https://identity.example.invalid/").is_err());
    assert!(IssuerOrigin::parse("https://identity.example.invalid/auth").is_err());
}

#[test]
fn one_set_of_registrations_has_one_rendering_whatever_order_it_arrives_in() {
    let forward = vec![
        registration("https://identity.example.invalid/realms/alpha", "alpha"),
        registration("https://identity.example.invalid/realms/beta", "beta"),
    ];
    let reversed: Vec<_> = forward.iter().rev().cloned().collect();

    let left = IssuerRegistryDocument::new(RegistryGeneration::FIRST, forward).unwrap();
    let right = IssuerRegistryDocument::new(RegistryGeneration::FIRST, reversed).unwrap();

    assert_eq!(left.canonical_json().unwrap(), right.canonical_json().unwrap());
}

#[test]
fn the_rendering_is_the_platform_contracts_keys() {
    let document = IssuerRegistryDocument::new(
        RegistryGeneration::FIRST,
        vec![registration(
            "https://identity.example.invalid/realms/alpha",
            "alpha",
        )],
    )
    .unwrap();

    assert_eq!(
        String::from_utf8(document.canonical_json().unwrap()).unwrap(),
        "{\n  \"schemaVersion\": 1,\n  \"generation\": 1,\n  \"issuers\": [\n    {\n      \
         \"issuer\": \"https://identity.example.invalid/realms/alpha\",\n      \"tenant\": \"alpha\"\n    \
         }\n  ]\n}\n"
    );
}

#[test]
fn an_issuer_registered_twice_is_refused_even_to_the_same_tenant() {
    let issuer = "https://identity.example.invalid/realms/alpha";

    let refused = IssuerRegistryDocument::new(
        RegistryGeneration::FIRST,
        vec![registration(issuer, "alpha"), registration(issuer, "alpha")],
    );

    assert!(matches!(refused, Err(RegistryRefusal::DuplicateIssuer { .. })));
}

#[test]
fn a_registry_at_generation_zero_or_with_an_unknown_key_does_not_parse() {
    let zero = r#"{"schemaVersion":1,"generation":0,"issuers":[]}"#;
    let extra = r#"{"schemaVersion":1,"generation":1,"issuers":[],"edge":1}"#;

    assert!(serde_json::from_str::<IssuerRegistryDocument>(zero).is_err());
    assert!(serde_json::from_str::<IssuerRegistryDocument>(extra).is_err());
}

#[test]
fn a_generation_only_moves_forward() {
    assert_eq!(RegistryGeneration::FIRST.next().get(), 2);
    assert!(RegistryGeneration::FIRST.next() > RegistryGeneration::FIRST);
}
