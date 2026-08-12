//! What each failure tells the caller — and what it must not.

use fabric_connector::{ConnectorError, ConnectorId};
use fabric_core::{DataSourceId, DataSourceName, TenantId};
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
        logical: DataSourceName::try_new("primary").unwrap(),
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
        logical: DataSourceName::try_new("audit").unwrap(),
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
    let error = DataApiError::Connector(ConnectorError::Unsupported {
        feature: "the contains comparison".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    assert!(error.public_message().contains("contains"));
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
