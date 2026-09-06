//! The container harness itself: does the stack come up, and does the real
//! handshake succeed where it must and refuse where it must.
//!
//! `docs/delivery.md`'s rule -- a slice is not complete until its primary
//! workflow is exercised through the real surface -- applies one layer down
//! here: before issue #62's composed acceptance test (a later slice) can
//! compose anything, the container harness it depends on has to actually
//! come up and answer. These three tests are that proof and nothing more;
//! the isolation and fail-closed assertions belong to the composed test.
//!
//! No `reqwest` or other new HTTP dependency: the real handshake is driven
//! through `fabric_connector_ndc::build_ndc_connector`, already a
//! dev-dependency of this crate, which performs the real `GET /capabilities`
//! and `GET /schema` calls this test needs. That is also the point --
//! registering a connector *is* the handshake this platform actually
//! performs at startup, not a stand-in for it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::collections::BTreeMap;
use std::time::Instant;

use fabric_connector::{ConnectorId, DataConnector as _};
use fabric_connector_ndc::{build_ndc_connector, NdcConnectorConfig};
use support::connector::ConnectorMode;
use support::gate::docker_available_or_skip;
use support::stack::Stack;

/// A connector configuration pointed at `endpoint`, naming `connection_name`
/// as the routing argument only when `connection_name_argument` is `Some`.
///
/// `fabric_connector_ndc::NdcConnectorConfig::for_test` exists for exactly
/// this shape but is crate-private to `fabric-connector-ndc`; every field of
/// the real struct is `pub`, so this crate builds it directly rather than
/// duplicating a constructor across the boundary.
fn config(endpoint: String, connection_name_argument: Option<&str>) -> NdcConnectorConfig {
    NdcConnectorConfig {
        id: ConnectorId::try_new("shared-postgres").unwrap(),
        endpoint,
        http_timeout_seconds: 10,
        http_connect_timeout_seconds: 5,
        connection_name_argument: connection_name_argument.map(str::to_owned),
        connection_string_argument: None,
        procedures: BTreeMap::new(),
    }
}

#[tokio::test]
async fn the_connector_reports_the_version_floor_and_the_shared_table() {
    let test_name = "the_connector_reports_the_version_floor_and_the_shared_table";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let started = Instant::now();
    let stack = Stack::up(ConnectorMode::Static);
    eprintln!("{test_name}: Stack::up(Static) took {:?}", started.elapsed());

    // The real handshake: GET /capabilities, then GET /schema, both against
    // the running connector. Succeeding at all already proves the connector
    // reported a specification version `check_version` accepts -- there is
    // no neutral surface to separately re-read "0.2.4" from once
    // negotiation has folded it into a pass/fail, and there should not be:
    // ADR 0001 keeps NDC vocabulary out of every crate above
    // fabric-connector-ndc, this crate included.
    let connector = build_ndc_connector(config(stack.connector_base_url.clone(), None), None)
        .await
        .expect("the static connector should negotiate with no routing configured");

    let articles = fabric_connector::CollectionName::try_new("articles").unwrap();
    assert!(
        connector.schema().collection(&articles).is_some(),
        "the real GET /schema should name the articles collection"
    );

    // The static connector's real schema has `request_arguments: null`
    // (§2.3 of the plan): a configuration that asks for name routing must
    // be refused, naming exactly that reason -- not merely refused for some
    // other one.
    // `NdcConnector` is not `Debug` (nothing in this crate needs it to be),
    // so the `Ok` side is discarded before `expect_err`, which requires it.
    let refused = build_ndc_connector(
        config(stack.connector_base_url.clone(), Some("connection_name")),
        None,
    )
    .await
    .map(|_connector| ())
    .expect_err("a routing argument the static connector never declared should be refused");
    assert!(
        refused.contains("declares no request-level arguments at all"),
        "expected the no-request-arguments refusal, got: {refused}"
    );
}

#[tokio::test]
async fn a_named_connector_declares_the_connection_argument() {
    let test_name = "a_named_connector_declares_the_connection_argument";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let started = Instant::now();
    let stack = Stack::up(ConnectorMode::Named);
    eprintln!("{test_name}: Stack::up(Named) took {:?}", started.elapsed());

    // The very configuration the static connector refuses above is accepted
    // here. The only way that can happen is the named connector's real
    // schema declaring `connection_name` as a request-level argument for
    // both queries and mutations -- see
    // `fabric_connector_ndc::registration::routing_arguments::check_routing_arguments`,
    // which requires both.
    build_ndc_connector(
        config(stack.connector_base_url.clone(), Some("connection_name")),
        None,
    )
    .await
    .expect("the named connector should declare connection_name for both request kinds");
}

#[tokio::test]
async fn both_tenants_rows_really_are_in_the_one_table() {
    let test_name = "both_tenants_rows_really_are_in_the_one_table";
    if !docker_available_or_skip(test_name) {
        return;
    }

    let started = Instant::now();
    let stack = Stack::up(ConnectorMode::Static);
    eprintln!("{test_name}: Stack::up(Static) took {:?}", started.elapsed());

    let count = stack.query_scalar("SELECT count(*) FROM articles;");
    assert_eq!(
        count, "2",
        "both tenants' rows should be present in the one shared table, read directly, not through \
         the connector whose predicate the composed test (a later slice) exists to check"
    );
}
