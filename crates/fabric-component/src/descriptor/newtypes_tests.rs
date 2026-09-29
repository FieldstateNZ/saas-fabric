//! Accept and refuse cases for every validated string a component
//! descriptor carries.
use super::{ComponentName, ComponentVersion, Digest, Repository, Role};

const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[test]
fn a_component_name_is_a_dns_label() {
    for good in ["reports", "saas-fabric", "a", "r2"] {
        assert!(ComponentName::try_new(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "Reports",
        "-reports",
        "reports-",
        "re_ports",
        "re.ports",
        &"a".repeat(64),
    ] {
        assert!(ComponentName::try_new(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_role_is_an_identifier() {
    for good in ["runtime", "controlPlane", "web_ui", "api-2"] {
        assert!(Role::try_new(good).is_ok(), "{good}");
    }
    for bad in ["", "2api", "-api", "api.web", "api web"] {
        assert!(Role::try_new(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_version_is_semver_without_build_metadata_or_a_prefix() {
    for good in [
        "0.0.0",
        "1.4.0",
        "10.20.30",
        "0.3.0-preview.14",
        "1.0.0-alpha",
        "1.0.0-0.3.7",
        "1.0.0-x-y.z--1",
    ] {
        assert!(ComponentVersion::try_new(good).is_ok(), "{good}");
    }
    let long = format!("1.0.0-{}", "a".repeat(123));
    assert_eq!(long.len(), 129);
    for bad in [
        "",
        "1",
        "1.0",
        "1.0.0.0",
        "01.0.0",
        "1.00.0",
        "1.0.01",
        "v1.0.0",
        "V1.0.0",
        "1.0.0+build",
        "1.0.0-rc+build",
        "1.0.0-",
        "1.0.0-01",
        "1.0.0-a..b",
        "1.0.0-a_b",
        "1.0.0 ",
        "-1.0.0",
        "a.b.c",
        &long,
    ] {
        assert!(ComponentVersion::try_new(bad).is_err(), "{bad}");
    }
    assert!(ComponentVersion::try_new(&long[..128]).is_ok());
}

#[test]
fn a_version_names_what_is_wrong_with_it() {
    let prefix = ComponentVersion::try_new("v1.0.0").unwrap_err().to_string();
    let build = ComponentVersion::try_new("1.0.0+build").unwrap_err().to_string();

    assert!(prefix.contains("v prefix"), "{prefix}");
    assert!(build.contains("build metadata"), "{build}");
}

#[test]
fn a_digest_is_lower_case_sha256() {
    assert!(Digest::try_new(format!("sha256:{HEX}")).is_ok());
    for bad in [
        String::new(),
        HEX.to_owned(),
        format!("sha256:{}", HEX.to_uppercase()),
        format!("sha512:{HEX}"),
        format!("sha256:{}", &HEX[1..]),
        format!("sha256:{HEX}0"),
        format!("sha256:{}g", &HEX[1..]),
        format!("SHA256:{HEX}"),
    ] {
        assert!(Digest::try_new(&bad).is_err(), "{bad}");
    }
}

#[test]
fn a_repository_is_written_in_full() {
    for good in [
        "ghcr.io/fieldstatenz/saas-fabric",
        "registry.example.com/acme/reports",
        "registry.example.com:5000/acme/reports",
        "localhost/reports",
        "localhost:5000/reports",
        "myregistry:8443/reports",
        "docker.io/library/nginx",
        "docker.io/acme/reports",
        "registry.example.com/a.b/c_d/e__f/g-h/i---j",
        "registry.example.com/a/b/c/d/e",
        "registry.example.com/0abc",
        "10.registry.example.com/acme/reports",
        "0x7f.example.com/acme/reports",
        "registry.example.0xcafe.io/acme/reports",
    ] {
        let repository = Repository::try_new(good).unwrap_or_else(|error| panic!("{good}: {error}"));
        assert_eq!(format!("{}/{}", repository.host(), repository.path()), good);
    }
}

#[test]
fn a_repository_names_its_host_and_path() {
    let repository = Repository::try_new("registry.example.com:5000/acme/reports").unwrap();

    assert_eq!(repository.host(), "registry.example.com:5000");
    assert_eq!(repository.path(), "acme/reports");
}

#[test]
fn every_implied_or_second_spelling_is_refused() {
    for (bad, says) in [
        ("nginx", "registry host"),
        ("acme/reports", "registry host"),
        ("Registry.Example.com/acme/reports", "lower case"),
        ("registry.example.com:443/acme/reports", "443"),
        ("registry.example.com:0/acme", "port"),
        ("registry.example.com:65536/acme", "port"),
        ("registry.example.com:05000/acme", "port"),
        ("registry.example.com:/acme", "port"),
        ("registry.example.com:+5000/acme", "port"),
        ("docker.io/nginx", "docker.io/library/nginx"),
        ("index.docker.io/library/nginx", "docker.io/..."),
        ("registry-1.docker.io/library/nginx", "docker.io/..."),
        ("registry.hub.docker.com/library/nginx", "docker.io/..."),
        ("10.0.0.1/acme/reports", "IP address"),
        ("10.0.0.1:5000/acme/reports", "IP address"),
        ("127.1:5000/acme/reports", "IP address"),
        ("0x7f000001:5000/acme/reports", "IP address"),
        ("0x7f.0.0.1/acme/reports", "IP address"),
        ("0x7f.1/acme/reports", "IP address"),
        ("registry.123/acme/reports", "IP address"),
        ("registry.0x/acme/reports", "IP address"),
        ("[::1]:5000/acme/reports", "DNS name"),
        ("https://ghcr.io/acme/reports", "scheme"),
        ("ghcr.io/acme/reports:1.0.0", "tag"),
        ("ghcr.io/acme/reports@sha256:abc", "digest"),
        ("ghcr.io/acme/reports/", "path"),
        ("ghcr.io/", "path"),
        ("ghcr.io//reports", "path"),
        ("ghcr.io/Acme/reports", "path"),
        ("ghcr.io/acme/-reports", "path"),
        ("ghcr.io/acme/reports-", "path"),
        ("ghcr.io/acme/re___ports", "path"),
        ("ghcr.io/acme/re..ports", "path"),
        ("ghcr.io/acme/re._ports", "path"),
        ("ghcr.io/acme/re-_ports", "path"),
        ("-ghcr.io/acme", "DNS name"),
        ("ghcr..io/acme", "DNS name"),
    ] {
        let error = Repository::try_new(bad).map(|_| ()).unwrap_err().to_string();
        assert!(error.contains(says), "{bad}: {error}");
    }
}

#[test]
fn a_repository_is_at_most_255_bytes() {
    let host = "registry.example.com/";
    let fits = format!("{host}{}", "a".repeat(255 - host.len()));
    let over = format!("{fits}a");

    assert!(Repository::try_new(&fits).is_ok());
    assert!(Repository::try_new(&over).is_err());
}

#[test]
fn serde_validates_on_the_way_in() {
    assert!(serde_json::from_str::<Repository>("\"docker.io/nginx\"").is_err());
    assert!(serde_json::from_str::<ComponentVersion>("\"v1.0.0\"").is_err());
    let role: Role = serde_json::from_str("\"runtime\"").unwrap();
    assert_eq!(serde_json::to_string(&role).unwrap(), "\"runtime\"");
}
