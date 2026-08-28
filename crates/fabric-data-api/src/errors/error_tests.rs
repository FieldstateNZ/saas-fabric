//! What each failure tells the caller — and what it must not.

use fabric_connector::{ComparisonOperator, ConnectorError, ConnectorId, UnsupportedFeature};
use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_identity::IdentityError;
use fabric_tenant_runtime::ResolveError;
use http::StatusCode;

use crate::{DataApiError, OperationKind};

fn tenant() -> TenantId {
    TenantId::try_new("ghost").unwrap()
}

fn connector() -> ConnectorId {
    ConnectorId::try_new("postgres").unwrap()
}

/// A transport source whose text names the infrastructure a caller may never
/// read (§2), so every assertion below doubles as a leak check.
fn revealing_source() -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::other(
        "sql-au-east-03.internal:5432 dropped the connection",
    ))
}

/// The failure a caller would see if the operation were a read.
fn reading(error: ConnectorError) -> DataApiError {
    DataApiError::connector(error, OperationKind::List)
}

/// The same failure raised while a non-idempotent insert was in flight.
fn writing(error: ConnectorError) -> DataApiError {
    DataApiError::connector(error, OperationKind::Create)
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
    let error = reading(ConnectorError::Rejected {
        connector: connector(),
        status: 400,
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
    let error = reading(
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
// only an operator can clear. For a *write* it is worse than wasteful: 503 is
// the status meshes and SDKs replay unasked, and `POST /data/{resource}` is
// not a request that may be replayed.

#[test]
fn an_unreachable_connector_is_a_retryable_503_for_a_read() {
    // Under the partial-failure startup policy (§35) a connector that has not
    // negotiated stays registered and is retried in the background, so this
    // is a routine transient state rather than an exceptional one. 500 would
    // tell every client the opposite.
    let error = reading(ConnectorError::Unreachable {
        connector: connector(),
        source: Box::new(std::io::Error::other("connection refused")),
    });

    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.code(), "connector_unavailable");
}

#[test]
fn an_unreachable_connector_still_names_no_infrastructure() {
    // 503 is a more informative status, which makes it worth re-checking that
    // it did not become a more informative *message*.
    let error = reading(ConnectorError::Unreachable {
        connector: connector(),
        source: revealing_source(),
    });

    let message = error.public_message();

    assert!(!message.contains("sql-au-east-03"), "{message}");
    assert!(!message.contains("5432"), "{message}");
}

// -- The three transport variants, read against write -----------------------
//
// They differ on the only question a non-idempotent write raises: did it
// happen? A read is entitled to ignore that difference, because nothing was
// mutated in any of the three. A write is not.

#[test]
fn every_transport_failure_stays_retryable_for_a_read() {
    for error in [
        ConnectorError::Unreachable {
            connector: connector(),
            source: revealing_source(),
        },
        ConnectorError::OutcomeUnknown {
            connector: connector(),
            source: revealing_source(),
        },
        ConnectorError::ResultLost {
            connector: connector(),
            source: revealing_source(),
        },
    ] {
        let error = reading(error);

        assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(error.code(), "connector_unavailable");
        assert_eq!(error.retry_after(), Some(5));
    }
}

#[test]
fn an_undelivered_write_is_the_one_transport_failure_that_invites_a_retry() {
    // `Unreachable` is built only from `is_connect() || is_builder()`, all of
    // which fail before a byte of the request is written.
    let error = writing(ConnectorError::Unreachable {
        connector: connector(),
        source: revealing_source(),
    });

    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.code(), "connector_unavailable");
    assert!(error.retry_after().is_some());
    assert!(error.public_message().contains("not carried out"));
}

#[test]
fn a_write_with_no_answer_is_a_502_that_does_not_invite_a_retry() {
    // The reported defect: a total-request timeout firing after the body was
    // sent used to answer 503, and something downstream replayed the insert.
    let error = writing(ConnectorError::OutcomeUnknown {
        connector: connector(),
        source: revealing_source(),
    });

    assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(error.code(), "write_outcome_unknown");
    assert_eq!(
        error.retry_after(),
        None,
        "the platform must not instruct a retry it cannot make safe"
    );
    assert!(error.public_message().contains("may or may not"));
}

#[test]
fn a_write_whose_result_was_lost_says_the_write_was_applied() {
    // A success status was read off the wire before this was built, so the
    // rows are in. Only the affected count is gone.
    let error = writing(ConnectorError::ResultLost {
        connector: connector(),
        source: revealing_source(),
    });

    assert_eq!(error.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(error.code(), "write_result_unavailable");
    assert_eq!(error.retry_after(), None);

    let message = error.public_message();
    assert!(message.contains("was applied"), "{message}");
    assert!(message.contains("do not retry"), "{message}");
}

#[test]
fn the_three_transport_variants_do_not_share_a_write_code() {
    // Clients branch on `code`, and these three need three different client
    // behaviours: retry, reconcile, accept. One code cannot carry that.
    let codes = [
        writing(ConnectorError::Unreachable {
            connector: connector(),
            source: revealing_source(),
        })
        .code(),
        writing(ConnectorError::OutcomeUnknown {
            connector: connector(),
            source: revealing_source(),
        })
        .code(),
        writing(ConnectorError::ResultLost {
            connector: connector(),
            source: revealing_source(),
        })
        .code(),
    ];

    let distinct: std::collections::BTreeSet<&str> = codes.into_iter().collect();

    assert_eq!(distinct.len(), codes.len(), "{codes:?}");
}

// -- The two failures raised only after a success status --------------------

#[test]
fn a_malformed_connector_response_stays_a_500() {
    // Deliberately *not* 503. A connector answering in a shape we cannot read
    // means a version skew or a misconfiguration; it reproduces on every
    // retry, so advertising it as transient would send clients into a loop
    // against a fault only an operator can clear.
    let error = reading(ConnectorError::MalformedResponse {
        connector: connector(),
        detail: "expected an object".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.public_message(), "internal error");
}

#[test]
fn a_malformed_response_to_a_write_does_not_tell_the_caller_it_failed() {
    // `decode_body` only ever builds this after a 2xx, which is why `effect()`
    // classifies it `Applied`. The status stays 500 because only an operator
    // can clear a version skew — but the message may not imply the rows are
    // absent, because they are not.
    let error = writing(ConnectorError::MalformedResponse {
        connector: connector(),
        detail: "expected an object".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let message = error.public_message();
    assert!(message.contains("was carried out"), "{message}");
    assert!(!message.contains("could not be executed"), "{message}");
}

#[test]
fn a_connector_rejection_stays_a_500() {
    let error = reading(ConnectorError::Rejected {
        connector: connector(),
        status: 400,
        message: "syntax error".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn a_write_the_backend_would_not_accept_is_reported_as_not_carried_out() {
    // 400 means the connector declined the request itself, so the platform can
    // say the one thing a caller holding a non-idempotent write most needs:
    // nothing landed. Before the status was carried this was `Unknown` and the
    // caller was sent to go and read their own data for no reason.
    let error = writing(ConnectorError::Rejected {
        connector: connector(),
        status: 400,
        message: "relation \"acme_prod.customers\" does not exist".to_owned(),
    });

    let message = error.public_message();

    assert!(message.contains("was not carried out"), "{message}");
    assert!(!message.contains("read the current state"), "{message}");
    assert!(!message.contains("acme_prod"), "{message}");
}

#[test]
fn a_write_refused_mid_flight_still_does_not_claim_it_did_not_happen() {
    // The other direction, and the one that keeps the first honest. A 409 is
    // 4xx, but the specification's example for it is a foreign key constraint
    // -- raised by the data source while writing -- and nothing makes a single
    // procedure atomic. Claiming "not carried out" here would be the data-loss
    // answer this whole classification exists to avoid.
    let error = writing(ConnectorError::Rejected {
        connector: connector(),
        status: 409,
        message: "duplicate key value violates constraint on acme_prod.customers".to_owned(),
    });

    let message = error.public_message();

    assert!(message.contains("read the current state"), "{message}");
    assert!(!message.contains("was not carried out"), "{message}");
    assert!(!message.contains("acme_prod"), "{message}");
}

#[test]
fn only_a_503_carries_a_retry_hint() {
    // The invariant that keeps the header honest: `Retry-After` is present
    // exactly where the platform is inviting a retry.
    let errors = [
        reading(ConnectorError::Unreachable {
            connector: connector(),
            source: revealing_source(),
        }),
        writing(ConnectorError::OutcomeUnknown {
            connector: connector(),
            source: revealing_source(),
        }),
        DataApiError::Resolve(ResolveError::RuntimeUnavailable),
        DataApiError::NotFound,
        DataApiError::PartiallyApplied {
            requested: 5,
            applied: 3,
        },
    ];

    for error in errors {
        assert_eq!(
            error.retry_after().is_some(),
            error.status() == StatusCode::SERVICE_UNAVAILABLE,
            "{}",
            error.code()
        );
    }
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
