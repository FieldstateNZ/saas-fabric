//! Reading `WWW-Authenticate` as registries actually write it.

use super::challenge::{challenges, parse, Scheme};

#[test]
fn ghcr_names_its_realm_service_and_scope() {
    let found = parse(
        r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:fieldstatenz/saas-fabric:pull""#,
    )
    .unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].scheme, Scheme::Bearer);
    assert_eq!(found[0].parameter("realm"), Some("https://ghcr.io/token"));
    assert_eq!(found[0].parameter("service"), Some("ghcr.io"));
    assert_eq!(
        found[0].parameter("scope"),
        Some("repository:fieldstatenz/saas-fabric:pull")
    );
}

#[test]
fn docker_hub_names_a_realm_on_another_host() {
    let found = parse(
        r#"Bearer realm="https://auth.docker.io/token",service="registry.docker.io",scope="repository:library/nginx:pull""#,
    )
    .unwrap();

    assert_eq!(found[0].parameter("realm"), Some("https://auth.docker.io/token"));
    assert_eq!(found[0].parameter("service"), Some("registry.docker.io"));
}

#[test]
fn gitlab_names_its_realm_on_the_instance_not_the_registry() {
    let found = parse(
        r#"Bearer realm="https://gitlab.com/jwt/auth",service="container_registry",scope="repository:group/project:pull""#,
    )
    .unwrap();

    assert_eq!(found[0].parameter("realm"), Some("https://gitlab.com/jwt/auth"));
    assert_eq!(found[0].parameter("service"), Some("container_registry"));
}

#[test]
fn harbor_offers_basic_and_bearer_in_one_header() {
    let found = parse(
        r#"Basic realm="harbor", Bearer realm="https://harbor.example.com/service/token",service="harbor-registry""#,
    )
    .unwrap();

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].scheme, Scheme::Basic);
    assert_eq!(found[0].parameter("realm"), Some("harbor"));
    assert_eq!(found[1].scheme, Scheme::Bearer);
    assert_eq!(
        found[1].parameter("realm"),
        Some("https://harbor.example.com/service/token")
    );
    assert_eq!(found[1].parameter("service"), Some("harbor-registry"));
}

#[test]
fn schemes_and_names_are_case_insensitive_and_spacing_is_free() {
    let found = parse(r#"bEaReR  REALM = "https://r.example/token" , Service=registry"#).unwrap();

    assert_eq!(found[0].scheme, Scheme::Bearer);
    assert_eq!(found[0].parameter("realm"), Some("https://r.example/token"));
    assert_eq!(found[0].parameter("service"), Some("registry"));
}

#[test]
fn a_quoted_value_keeps_its_commas_equals_and_escaped_quotes() {
    let found = parse(
        r#"Bearer realm="https://r.example/token",error="insufficient_scope",error_description="a \"quoted\", realm=\"x\" word""#,
    )
    .unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].parameter("realm"), Some("https://r.example/token"));
    assert_eq!(
        found[0].parameter("error_description"),
        Some(r#"a "quoted", realm="x" word"#)
    );
}

#[test]
fn a_scheme_with_no_parameters_is_a_challenge() {
    let found = parse("Basic, Bearer realm=\"https://r.example/token\"").unwrap();

    assert_eq!(found[0].scheme, Scheme::Basic);
    assert!(found[0].parameters.is_empty());
    assert_eq!(found[1].scheme, Scheme::Bearer);
}

#[test]
fn a_malformed_header_offers_nothing() {
    for malformed in [
        r#"Bearer realm="https://ghcr.io/token"#,
        r#"Bearer realm="x" "junk""#,
        "Bearer realm=",
        "\"Bearer\"",
    ] {
        assert_eq!(parse(malformed), None, "{malformed}");
    }
}

#[test]
fn another_scheme_is_read_and_never_mistaken_for_one_answered() {
    let found = parse(r#"Negotiate, Bearer realm="https://r.example/token""#).unwrap();

    assert_eq!(found[0].scheme, Scheme::Other("negotiate".to_owned()));
    assert_eq!(found[1].scheme, Scheme::Bearer);
}

#[test]
fn every_header_is_read_and_a_malformed_one_is_skipped() {
    let mut headers = reqwest::header::HeaderMap::new();
    for value in [r#"Bearer realm="unterminated"#, r#"Basic realm="r""#] {
        headers.append(reqwest::header::WWW_AUTHENTICATE, value.parse().unwrap());
    }

    let found = challenges(&headers);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].scheme, Scheme::Basic);
}
