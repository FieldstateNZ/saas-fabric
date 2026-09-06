//! A `KeycloakConfig` for tests, with the audience always stated by hand.
//!
//! `KeycloakConfig` carries no `Default` impl (see its own rustdoc): nothing
//! may construct one without naming an audience, tests included. This is the
//! one place that states the ordinary values every other field falls back to
//! in production, so a test asking for "the usual config, pointed at this
//! fake, with this audience" has one function to call rather than four field
//! names to restate at every call site.

use fabric_keycloak::KeycloakConfig;

/// Builds a config for a fake Keycloak at `base_url`, asserting `audience`.
///
/// `admin_realm`, `client_id` and `http_timeout_seconds` take the same
/// values `KeycloakConfig`'s own serde defaults would supply. That is not a
/// second set of defaults to keep in sync with the first — it is this
/// function's opinion of what an ordinary adapter test does not need to
/// vary, and a test that does vary one still builds `KeycloakConfig` itself
/// rather than asking this helper to grow a parameter for it.
pub fn config_for_tests(base_url: &str, audience: &str) -> KeycloakConfig {
    KeycloakConfig {
        base_url: base_url.to_owned(),
        admin_realm: "master".to_owned(),
        client_id: "saas-fabric".to_owned(),
        http_timeout_seconds: 10,
        audience: audience.to_owned(),
    }
}
