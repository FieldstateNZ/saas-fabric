//! Each kind's settings, and what an endpoint may be.

use super::{AddressPolicy, Credential, RealmRule, RegistrySecret, RegistrySettings};

#[test]
fn docker_hub_names_repositories_docker_io_and_is_served_elsewhere() {
    let hub = RegistrySettings::docker_hub();

    assert_eq!(hub.naming_host, "docker.io");
    assert_eq!(hub.endpoint, "https://registry-1.docker.io/");
    assert_eq!(
        hub.realm,
        RealmRule::Fixed {
            realm: "https://auth.docker.io/token".to_owned(),
            service: "registry.docker.io".to_owned(),
        }
    );
    assert_eq!(hub.address, AddressPolicy::PublicOnly);
    assert!(!hub.honours_basic);
}

#[test]
fn a_distribution_registry_is_named_by_its_host_and_port() {
    let plain =
        RegistrySettings::distribution("https://registry.example", RealmRule::FollowChallenge).unwrap();
    let ported =
        RegistrySettings::distribution("https://registry.example:5000/", RealmRule::FollowChallenge).unwrap();
    let default_port =
        RegistrySettings::distribution("https://registry.example:443", RealmRule::FollowChallenge).unwrap();

    assert_eq!(plain.naming_host, "registry.example");
    assert_eq!(ported.naming_host, "registry.example:5000");
    assert_eq!(default_port.naming_host, "registry.example");
    assert!(plain.honours_basic);
    assert_eq!(plain.address, AddressPolicy::PublicOnly);
}

#[test]
fn a_distribution_endpoint_is_an_https_origin_named_by_a_host() {
    for (endpoint, says) in [
        ("http://registry.example", "HTTPS"),
        ("https://10.0.0.1", "IP address"),
        ("https://[fd00::1]:5000", "IP address"),
        ("https://registry.example/v2", "path"),
        ("https://user:secret@registry.example", "credential"),
        ("https://registry.example/?secret", "query"),
        ("https://registry.example/#secret", "fragment"),
        ("not a url", "not a URL"),
    ] {
        let Err(message) = RegistrySettings::distribution(endpoint, RealmRule::FollowChallenge) else {
            panic!("{endpoint} is refused");
        };
        assert!(message.contains(says), "{endpoint}: {message}");
        assert!(message.contains("endpoint"), "{message}");
        assert!(
            !message.contains("secret") && !message.contains("10.0.0.1"),
            "{message}"
        );
    }
}

#[test]
fn a_distribution_realm_is_never_fixed() {
    let fixed = RealmRule::Fixed {
        realm: "https://ghcr.io/token".to_owned(),
        service: "ghcr.io".to_owned(),
    };

    assert!(RegistrySettings::distribution("https://registry.example", fixed).is_err());
}

#[test]
fn a_credential_needs_a_username_basic_can_carry_and_a_secret() {
    let secret = || RegistrySecret::new("hunter2");

    assert!(Credential::new("", secret(), ["ghcr.io/a/b"]).is_err());
    assert!(Credential::new("user:name", secret(), ["ghcr.io/a/b"]).is_err());
    assert!(Credential::new("user", RegistrySecret::new(" "), ["ghcr.io/a/b"]).is_err());
    let Err(message) = Credential::new("user:name", secret(), ["ghcr.io/a/b"]) else {
        panic!("refused");
    };
    assert!(
        !message.contains("user:name") && !message.contains("hunter2"),
        "{message}"
    );
}

#[test]
fn the_secret_is_never_debug_printed() {
    let credential = Credential::new("brett", RegistrySecret::new("hunter2"), ["ghcr.io/a/b"]).unwrap();
    let settings = RegistrySettings::ghcr().with_credential(credential.clone());

    for printed in [
        format!("{credential:?}"),
        format!("{settings:?}"),
        format!("{settings:#?}"),
    ] {
        assert!(!printed.contains("hunter2"), "{printed}");
        assert!(printed.contains("redacted"), "{printed}");
    }
}

#[test]
fn only_loopback_may_be_served_from_and_only_by_a_test() {
    assert!(RegistrySettings::ghcr().serve_from("http://127.0.0.1:1").is_ok());
    assert!(RegistrySettings::ghcr().serve_from("http://localhost:1").is_ok());
    assert!(RegistrySettings::ghcr()
        .serve_from("http://registry.example")
        .is_err());
}
