//! What each failure tells the caller — and what it must not.

use fabric_connector::{ComparisonOperator, ConnectorError, ConnectorId, UnsupportedFeature};
use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_identity::IdentityError;
use fabric_tenant_runtime::ResolveError;
use http::StatusCode;

use crate::DataApiError;

fn tenant() -> TenantId {
    TenantId::try_new("ghost").unwrap()
}

#[test]
fn an_unprimed_runtime_is_a_retryable_503() {
    let error = DataApiError::Resolve(ResolveError::RuntimeUnavailable);

    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.code(), "runtime_unavailable");
}

#[test]
fn an_unknown_tenant_is_403_not_404() {
    // 404 would let a caller enumerate which tenants exist.
    let error = DataApiError::Resolve(ResolveError::UnknownTenant(tenant()));

    assert_eq!(error.status(), StatusCode::FORBIDDEN);
}

#[test]
fn an_unknown_tenant_message_does_not_echo_the_tenant_back() {
    let error = DataApiError::Resolve(ResolveError::UnknownTenant(tenant()));

    assert!(!error.public_message().contains("ghost"));
}

#[test]
fn a_missing_data_source_never_names_the_data_source_to_the_caller() {
    // The id is platform topology. Internally it is logged in full.
    let error = DataApiError::Resolve(ResolveError::MissingDataSource {
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        data_source: DataSourceId::try_new("sql-au-east-03").unwrap(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.public_message(), "internal error");
    assert!(!error.public_message().contains("sql-au-east-03"));
}

#[test]
fn an_unbound_logical_data_source_is_an_internal_error() {
    let error = DataApiError::Resolve(ResolveError::UnboundDataSource {
        tenant: TenantId::try_new("acme").unwrap(),
        logical: LogicalDataSourceName::try_new("audit").unwrap(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.public_message(), "internal error");
}

#[test]
fn a_read_only_data_source_produces_a_405_that_names_no_placement() {
    let error = DataApiError::ResourceIsReadOnly {
        resource: "customers".to_owned(),
    };

    assert_eq!(error.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(error.code(), "read_only");

    let message = error.public_message();
    assert!(message.contains("read-only"));
    // Which DataSource, and why, stays internal.
    assert!(!message.contains("replica"));
    assert!(!message.contains("data source"));
}

#[test]
fn a_connector_rejection_never_reaches_the_caller_verbatim() {
    // Connector text names physical tables and servers.
    let error = DataApiError::Connector(ConnectorError::Rejected {
        connector: ConnectorId::try_new("postgres").unwrap(),
        message: "relation \"acme_prod.customers\" does not exist on sql-au-east-03".to_owned(),
    });

    let message = error.public_message();

    assert_eq!(message, "internal error");
    assert!(!message.contains("acme_prod"));
    assert!(!message.contains("sql-au-east-03"));
}

#[test]
fn an_unsupported_operation_is_explained_because_it_names_no_infrastructure() {
    // The refusal carries physical detail alongside the capability name. The
    // caller gets the name; the detail is unreachable from `public_message`,
    // because `RefusalDetail` has no `Display` to reach it through.
    let error = DataApiError::Connector(
        UnsupportedFeature::Comparison(ComparisonOperator::Contains)
            .refused_because("customer_records_v2.name has no contains operator"),
    );

    assert_eq!(error.status(), StatusCode::BAD_REQUEST);

    let message = error.public_message();

    assert_eq!(
        message,
        "this operation is not supported: the contains comparison"
    );
    assert!(!message.contains("customer_records_v2"));
}

#[test]
fn a_missing_tenant_claim_is_a_401() {
    let error = DataApiError::Identity(IdentityError::MissingTenantClaim {
        claim: "tenant_id".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::UNAUTHORIZED);
}

#[test]
fn a_tenant_header_attempt_is_a_400() {
    let error = DataApiError::Identity(IdentityError::TenantHeaderPresent {
        header: "x-tenant-id",
    });

    assert_eq!(error.status(), StatusCode::BAD_REQUEST);
}

// -- Which connector failures are retryable, and which are not --------------
//
// The distinction is the whole reason `is_internal` is not a single status.
// A caller, a proxy and an SDK retry policy all branch on 5xx-vs-503, so
// getting it wrong either wastes a recoverable request or hammers a fault
// only an operator can clear.

#[test]
fn an_unreachable_connector_is_a_retryable_503() {
    // Under the partial-failure startup policy (§35) a connector that has not
    // negotiated stays registered and is retried in the background, so this
    // is a routine transient state rather than an exceptional one. 500 would
    // tell every client the opposite.
    let error = DataApiError::Connector(ConnectorError::Unreachable {
        connector: ConnectorId::try_new("postgres").unwrap(),
        source: Box::new(std::io::Error::other("connection refused")),
    });

    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn an_unreachable_connector_still_names_no_infrastructure() {
    // 503 is a more informative status, which makes it worth re-checking that
    // it did not become a more informative *message*.
    let error = DataApiError::Connector(ConnectorError::Unreachable {
        connector: ConnectorId::try_new("postgres").unwrap(),
        source: Box::new(std::io::Error::other(
            "failed to connect to sql-au-east-03.internal:5432",
        )),
    });

    let message = error.public_message();

    assert!(!message.contains("sql-au-east-03"), "{message}");
    assert!(!message.contains("5432"), "{message}");
}

#[test]
fn a_malformed_connector_response_stays_a_500() {
    // Deliberately *not* 503. A connector answering in a shape we cannot read
    // means a version skew or a misconfiguration; it reproduces on every
    // retry, so advertising it as transient would send clients into a loop
    // against a fault only an operator can clear.
    let error = DataApiError::Connector(ConnectorError::MalformedResponse {
        connector: ConnectorId::try_new("postgres").unwrap(),
        detail: "expected an object".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn a_connector_rejection_stays_a_500() {
    let error = DataApiError::Connector(ConnectorError::Rejected {
        connector: ConnectorId::try_new("postgres").unwrap(),
        message: "syntax error".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn an_unenforceable_isolation_model_is_a_500_not_a_503() {
    // A tenant bound to a shared DataSource under structural isolation is a
    // reconciliation error: nothing the caller sent is wrong, and nothing
    // resolves it on its own. 503 would invite a retry storm from one
    // misconfigured binding.
    let error = DataApiError::Resolve(ResolveError::IsolationNotEnforceable {
        tenant: tenant(),
        data_source: DataSourceId::try_new("shared-postgres-02").unwrap(),
        isolation: "schema",
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.code(), "internal");
}

#[test]
fn an_unenforceable_isolation_model_names_no_infrastructure_to_the_caller() {
    let error = DataApiError::Resolve(ResolveError::IsolationNotEnforceable {
        tenant: tenant(),
        data_source: DataSourceId::try_new("shared-postgres-02").unwrap(),
        isolation: "schema",
    });

    assert_eq!(error.public_message(), "internal error");
}
