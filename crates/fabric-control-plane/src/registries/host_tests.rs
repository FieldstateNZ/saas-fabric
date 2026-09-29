//! What a registry may be named, and where a `distribution` one may be
//! served.

use super::endpoint::distribution;
use super::RegistryHost;

#[test]
fn a_host_is_a_lower_case_dns_name_with_an_optional_port() {
    for host in [
        "ghcr.io",
        "docker.io",
        "registry.example.com:5000",
        "localhost:5000",
    ] {
        assert_eq!(RegistryHost::parse(host).unwrap().as_str(), host);
    }
}

#[test]
fn a_host_is_never_an_address_a_path_or_a_second_spelling() {
    for host in [
        "",
        "Registry.Example.com",
        "10.0.0.1",
        "[::1]:5000",
        "registry.example.com/acme",
        "registry.example.com:443",
        "index.docker.io",
        "user@registry.example.com",
        "nodots",
    ] {
        assert!(RegistryHost::parse(host).is_err(), "{host:?} was accepted");
    }
}

#[test]
fn a_distribution_endpoint_is_an_https_origin_named_by_its_host() {
    let (host, endpoint) = distribution("https://registry.example.com:5000/").unwrap();

    assert_eq!(host.as_str(), "registry.example.com:5000");
    assert_eq!(endpoint, "https://registry.example.com:5000");
}

#[test]
fn a_distribution_endpoint_carries_nothing_but_its_origin() {
    for (endpoint, rule) in [
        ("http://registry.example.com", "HTTPS"),
        ("https://registry.example.com/v2", "path"),
        ("https://registry.example.com/?a=b", "query"),
        ("https://registry.example.com/#x", "fragment"),
        ("https://user:pass@registry.example.com", "user information"),
        ("https://10.0.0.1", "IP address"),
        ("https://[::1]", "IP address"),
        ("https://169.254.169.254", "IP address"),
        ("registry.example.com", "HTTPS"),
    ] {
        let message = distribution(endpoint).unwrap_err();
        assert!(message.contains(rule), "{endpoint}: {message}");
        assert!(
            !message.contains("pass"),
            "a refusal must not echo a credential: {message}"
        );
    }
}

#[test]
fn ghcr_and_docker_hub_are_only_ever_registered_as_their_own_kinds() {
    assert!(distribution("https://ghcr.io").unwrap_err().contains("ghcr"));
    assert!(distribution("https://docker.io")
        .unwrap_err()
        .contains("dockerHub"));
    assert!(distribution("https://registry-1.docker.io").is_err());
}
