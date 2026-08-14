//! Whether a connector can actually carry the routing it is configured for.
//!
//! Every case here is a `200` in the field. Nothing about a request routed to
//! the wrong database looks different from one routed correctly, so the only
//! place to catch it is before the connector serves anything at all.

use std::collections::BTreeMap;

use super::routing_arguments::check_routing_arguments;
use crate::wire::NdcSchemaResponse;
use crate::NdcConnectorConfig;

/// A connector configured for both routing modes — the example configuration's
/// shape, and the one with the most to check.
fn config() -> NdcConnectorConfig {
    NdcConnectorConfig::for_test(BTreeMap::new())
}

/// A connector configured for neither: one database, every tenant on
/// [`ConnectionSelector::Default`](fabric_connector::ConnectionSelector).
fn unrouted_config() -> NdcConnectorConfig {
    NdcConnectorConfig {
        connection_name_argument: None,
        connection_string_argument: None,
        ..config()
    }
}

/// A connector configured for name routing only.
fn name_routed_config() -> NdcConnectorConfig {
    NdcConnectorConfig {
        connection_string_argument: None,
        ..config()
    }
}

fn schema(request_arguments: &str) -> NdcSchemaResponse {
    serde_json::from_str(&format!(
        r#"{{
            "scalar_types": {{}}, "object_types": {{}}, "collections": [],
            "functions": [], "procedures": []{request_arguments}
        }}"#
    ))
    .unwrap()
}

/// What `ndc-postgres` v3.1.0 returns when statically configured:
/// `ConnectionSettings::Static { .. } => None`.
fn static_postgres_schema() -> NdcSchemaResponse {
    schema("")
}

/// Declares `argument` for both queries and mutations, as a name-mode
/// `ndc-postgres` does for `connection_name`.
fn declaring(argument: &str) -> NdcSchemaResponse {
    schema(&format!(
        r#", "request_arguments": {{
            "query_arguments": {{"{argument}": {{"type": {{"type": "named", "name": "text"}}}}}},
            "mutation_arguments": {{"{argument}": {{"type": {{"type": "named", "name": "text"}}}}}},
            "relational_query_arguments": {{}}
        }}"#
    ))
}

// -- The blocking case: a Static connector accepted every tenant ------------

#[test]
fn a_connector_declaring_no_request_arguments_is_refused_when_routing_is_configured() {
    let error = check_routing_arguments(&config(), &static_postgres_schema()).unwrap_err();

    assert!(error.contains("connection_name_argument"), "{error}");
    assert!(error.contains("declares no request-level arguments"), "{error}");
}

#[test]
fn an_explicit_null_request_arguments_member_is_the_same_refusal() {
    // `anyOf [RequestLevelArguments, null]` -- an explicit null says the same
    // thing as omitting the key, and must not read as "declared".
    let error = check_routing_arguments(&config(), &schema(r#", "request_arguments": null"#)).unwrap_err();

    assert!(error.contains("declares no request-level arguments"), "{error}");
}

// -- The second proven leak: a mode the connector cannot serve --------------

#[test]
fn a_name_only_connector_is_refused_when_secret_routing_is_also_configured() {
    // A name-mode `ndc-postgres` with a fallback pool looks only for
    // `connection_name`, so a secret-routed tenant would silently get the
    // fallback database.
    let error = check_routing_arguments(&config(), &declaring("connection_name")).unwrap_err();

    assert!(error.contains("connection_string_argument"), "{error}");
    assert!(error.contains("queries or mutations"), "{error}");
}

#[test]
fn a_name_only_connector_is_accepted_when_only_name_routing_is_configured() {
    // The same connector, honestly configured. This is why the check reads the
    // configuration rather than demanding both arguments unconditionally --
    // otherwise no real `ndc-postgres` deployment would ever start.
    assert_eq!(
        check_routing_arguments(&name_routed_config(), &declaring("connection_name")),
        Ok(())
    );
}

// -- Half-declared: one request kind but not the other ---------------------

#[test]
fn an_argument_declared_for_queries_but_not_mutations_is_refused() {
    // The direction that reads the right tenant's rows and writes into
    // somebody else's.
    let half = schema(
        r#", "request_arguments": {
            "query_arguments": {"connection_name": {"type": {"type": "named", "name": "text"}}},
            "mutation_arguments": {},
            "relational_query_arguments": {}
        }"#,
    );

    let error = check_routing_arguments(&name_routed_config(), &half).unwrap_err();

    assert!(error.contains("mutations"), "{error}");
    assert!(!error.contains("queries or"), "{error}");
}

#[test]
fn an_argument_declared_for_mutations_but_not_queries_is_refused() {
    let half = schema(
        r#", "request_arguments": {
            "query_arguments": {},
            "mutation_arguments": {"connection_name": {"type": {"type": "named", "name": "text"}}},
            "relational_query_arguments": {}
        }"#,
    );

    let error = check_routing_arguments(&name_routed_config(), &half).unwrap_err();

    assert!(error.contains("queries"), "{error}");
    assert!(!error.contains("or mutations"), "{error}");
}

// -- A connector using its own argument names ------------------------------

#[test]
fn the_check_follows_the_configured_name_not_a_hardcoded_one() {
    // Nothing in the specification fixes these names; `connection_name` is
    // just what `ndc-postgres` chose.
    let config = NdcConnectorConfig {
        connection_name_argument: Some("tenant_db".to_owned()),
        connection_string_argument: None,
        ..config()
    };

    assert_eq!(check_routing_arguments(&config, &declaring("tenant_db")), Ok(()));
    assert!(check_routing_arguments(&config, &declaring("connection_name")).is_err());
}

// -- No routing configured -> nothing to check -----------------------------

#[test]
fn a_connector_with_no_routing_configured_needs_no_declaration() {
    // One database, every tenant on the default connection. There is nothing
    // to route, so a Static connector is exactly right here.
    assert_eq!(
        check_routing_arguments(&unrouted_config(), &static_postgres_schema()),
        Ok(())
    );
}
