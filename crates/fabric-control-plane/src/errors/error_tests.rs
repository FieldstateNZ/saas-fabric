//! Tests for how a refusal is presented.

use axum::response::IntoResponse as _;
use fabric_client_model::{ClientId, DesiredStateError, RealmName};
use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_platform_management::{
    DataSourceRule, DesiredStateError as PlatformDesiredStateError, PlacementRefusal, PlatformError,
};
use http::StatusCode;

use crate::operator::OperatorAuthError;
use crate::repository::RepositoryError;
use crate::ControlPlaneError;

fn client() -> ClientId {
    ClientId::try_new("acme").unwrap()
}

fn logical() -> LogicalDataSourceName {
    LogicalDataSourceName::try_new("primary").unwrap()
}

fn tenant() -> TenantId {
    TenantId::try_new("acme").unwrap()
}

fn data_source() -> DataSourceId {
    DataSourceId::try_new("shared-postgres-nz-01").unwrap()
}

#[test]
fn every_failure_has_its_own_machine_code() {
    let errors = [
        ControlPlaneError::Unauthenticated(OperatorAuthError::Missing),
        ControlPlaneError::OperatorRefused,
        ControlPlaneError::UnknownClient(client()),
        ControlPlaneError::InvalidRequest(DesiredStateError::MissingField { field: "spec" }),
        ControlPlaneError::InvalidDesiredState {
            client: client(),
            source: DesiredStateError::MissingField { field: "spec" },
        },
        ControlPlaneError::RevisionRequired,
        ControlPlaneError::RevisionConflict,
        ControlPlaneError::ClientExists { id: client() },
        ControlPlaneError::RealmImmutable {
            current: RealmName::try_new("acme").unwrap(),
        },
        ControlPlaneError::RepositoryUnavailable,
        ControlPlaneError::RepositoryDenied,
        ControlPlaneError::RepositoryRejected,
        ControlPlaneError::IntegrationRefused("no such repository".to_owned()),
        ControlPlaneError::IntegrationMoved,
        ControlPlaneError::InvalidDataSource(DataSourceRule::SharedNeedsDiscriminator),
        ControlPlaneError::LogicalDataSourceNotDeclared { logical: logical() },
        ControlPlaneError::Platform(PlatformError::DataSourceInUse {
            id: data_source(),
            tenants: vec![tenant()],
        }),
        ControlPlaneError::PublicationNotConfigured,
        ControlPlaneError::PublicationRunning,
    ];

    let mut codes: Vec<&str> = errors.iter().map(ControlPlaneError::code).collect();
    let total = codes.len();
    codes.sort_unstable();
    codes.dedup();

    assert_eq!(
        codes.len(),
        total,
        "two failures share a code, so a client cannot tell them apart"
    );
}

#[test]
fn a_refused_bearer_is_distinct_from_having_none_and_says_nothing_about_why() {
    // Both are 401 -- the operator holds no usable identity either way -- but
    // a console (ADR 0024's gateway probe) needs to tell them apart from the
    // machine code, and the message must not become a place the token or the
    // verification failure leaks back to the browser.
    let missing = ControlPlaneError::Unauthenticated(OperatorAuthError::Missing);
    let refused = ControlPlaneError::OperatorRefused;

    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(missing.code(), "unauthenticated");
    assert_eq!(refused.code(), "operator_refused");
    assert_ne!(missing.code(), refused.code());
    assert_eq!(refused.public_message(), "A bearer was presented and refused.");
}

#[test]
fn a_stale_revision_is_a_conflict_and_a_missing_one_is_a_precondition() {
    // Different problems, different remedies: one means redo your edit, the
    // other means send the header.
    assert_eq!(ControlPlaneError::RevisionConflict.status(), StatusCode::CONFLICT);
    assert_eq!(
        ControlPlaneError::RevisionRequired.status(),
        StatusCode::PRECONDITION_REQUIRED
    );
}

#[test]
fn an_unreadable_stored_document_is_a_server_error_not_a_bad_request() {
    // Nothing the operator sent caused it, and no correction to their request
    // will fix it.
    let error = ControlPlaneError::InvalidDesiredState {
        client: client(),
        source: DesiredStateError::MissingField { field: "spec" },
    };

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn an_unreadable_stored_catalogue_shares_the_client_documents_code_and_status() {
    // Deliberate: a console reading either has one thing to do, stop and do
    // not retry, so there is no second code for it to branch on. See
    // `ControlPlaneError::InvalidCatalogue`'s rustdoc.
    let client_document = ControlPlaneError::InvalidDesiredState {
        client: client(),
        source: DesiredStateError::MissingField { field: "spec" },
    };
    let catalogue = ControlPlaneError::InvalidCatalogue {
        source: DesiredStateError::MissingField { field: "spec" },
    };

    assert_eq!(catalogue.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(catalogue.code(), client_document.code());
    assert_eq!(catalogue.code(), "desired_state_invalid");
}

#[test]
fn a_refused_platform_credential_is_not_advertised_as_transient() {
    // 503 would invite a retry storm over a secret that will still be wrong.
    assert_eq!(
        ControlPlaneError::RepositoryDenied.status(),
        StatusCode::BAD_GATEWAY
    );
    assert_eq!(
        ControlPlaneError::RepositoryUnavailable.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[test]
fn a_repository_failure_detail_does_not_survive_translation() {
    // The detail may name a branch, a path, or an upstream body. It is logged
    // by the adapter and dropped here.
    let error = ControlPlaneError::from_repository(RepositoryError::Unavailable {
        detail: "github: 500 while reading clients/acme/client.yaml".to_owned(),
    });

    assert!(!error.public_message().contains("clients/acme"));
    assert!(!error.public_message().contains("github"));
}

#[test]
fn a_decision_taken_against_state_that_moved_is_a_conflict_not_an_outage() {
    // It reaches an operator from their own pause, resume or rollback click:
    // the component's state moved, or the platform was rebound to another
    // repository, between the read and the write. Falling to the catch-all made
    // it a 503 with a `Retry-After` and a server-error log line — telling them
    // to retry something that would be refused identically, and recording their
    // click as a platform fault.
    let error = ControlPlaneError::Platform(PlatformError::DesiredState(PlatformDesiredStateError::Conflict));

    assert_eq!(error.status(), StatusCode::CONFLICT);
    assert_eq!(error.code(), "platform_state_moved");
}

#[test]
fn a_stale_platform_decision_is_not_advertised_as_retryable() {
    // The header is what a console and an impatient client act on. 409 never
    // carries it, so this is really a check that the arm above did not land in
    // the 503 group by accident.
    let response =
        ControlPlaneError::Platform(PlatformError::DesiredState(PlatformDesiredStateError::Conflict))
            .into_response();

    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(
        response.headers().get(http::header::RETRY_AFTER).is_none(),
        "a decision that has to be retaken is not something to retry unchanged"
    );
}

#[test]
fn a_broken_held_document_is_not_advertised_as_retryable_either() {
    // 500 never carries `Retry-After` in this API (`response.rs` attaches
    // it only to `503`), but a coherence problem in a document this
    // platform authored is exactly the case that must never look
    // retryable: no amount of waiting fixes a file nobody has corrected.
    let response = ControlPlaneError::Platform(PlatformError::InvalidHeldDataSources {
        detail: "shared-postgres-nz-01 is declared more than once".to_owned(),
    })
    .into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        response.headers().get(http::header::RETRY_AFTER).is_none(),
        "a broken document is not something retrying will fix"
    );
}

#[test]
fn no_publication_target_is_a_not_found_not_an_outage() {
    // Only a config edit and a restart change this -- the same species of
    // absence `PlatformNotManaged` is, not a transient one, so it shares
    // that variant's 404 and never carries `Retry-After`.
    let response = ControlPlaneError::PublicationNotConfigured.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(
        response.headers().get(http::header::RETRY_AFTER).is_none(),
        "nothing here will be different in five seconds"
    );
}

#[test]
fn an_integration_that_moved_is_a_conflict_rather_than_a_refusal_or_an_outage() {
    // It reaches an operator from their own click on a repository: a disconnect
    // or another operator's rebind landed between the page they read and the
    // choice they made. Not a 400 -- the request was well-formed and would have
    // been applied a moment earlier -- and not a 503, which would advertise an
    // immediate retry that would be refused identically.
    let error = ControlPlaneError::IntegrationMoved;

    assert_eq!(error.status(), StatusCode::CONFLICT);
    assert_eq!(error.code(), "integration_moved");
    assert!(
        error
            .into_response()
            .headers()
            .get(http::header::RETRY_AFTER)
            .is_none(),
        "a choice that has to be made again is not something to retry unchanged"
    );
}

#[test]
fn a_duplicate_client_id_is_a_conflict_with_its_own_code_not_a_stale_revision() {
    // `create` has no prior read to have gone stale, so its `Conflict` can
    // only mean the id is taken — a different event from
    // `RevisionConflict`, and one a console needs its own code to show the
    // right message for.
    let error = ControlPlaneError::ClientExists { id: client() };

    assert_eq!(error.status(), StatusCode::CONFLICT);
    assert_eq!(error.code(), "client_exists");
    assert_ne!(error.code(), ControlPlaneError::RevisionConflict.code());
}

#[test]
fn a_missing_client_is_reported_as_unknown_rather_than_as_a_repository_failure() {
    let error = ControlPlaneError::from_repository(RepositoryError::NotFound { client: client() });

    assert_eq!(error.status(), StatusCode::NOT_FOUND);
    assert_eq!(error.code(), "unknown_client");
}

#[test]
fn a_declared_data_source_rule_is_unprocessable_with_the_rules_own_words() {
    // The request was understood; what it asked for breaks ADR 0023 part 1,
    // and the fix is to change what was submitted rather than to retry it —
    // the same 422 `DocumentTooLarge` answers for the same reason.
    let error = ControlPlaneError::InvalidDataSource(DataSourceRule::SharedNeedsDiscriminator);

    assert_eq!(error.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error.code(), "invalid_data_source");
    assert_eq!(
        error.public_message(),
        DataSourceRule::SharedNeedsDiscriminator.to_string()
    );
}

#[test]
fn a_platform_invalid_data_source_wrapped_generically_still_answers_the_structural_arm() {
    // A handler that lets `?` wrap `PlatformError::InvalidDataSource` in
    // `ControlPlaneError::Platform`, rather than unwrapping it itself, must
    // still answer the same 422 `invalid_data_source` the direct variant
    // above does -- the structural arms in `status_mapping::platform` and
    // `status_mapping::codes` are what make the handler's own translation
    // unnecessary.
    let direct = ControlPlaneError::InvalidDataSource(DataSourceRule::SharedNeedsDiscriminator);
    let wrapped = ControlPlaneError::Platform(PlatformError::InvalidDataSource(
        DataSourceRule::SharedNeedsDiscriminator,
    ));

    assert_eq!(wrapped.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(wrapped.code(), "invalid_data_source");
    assert_eq!(wrapped.code(), direct.code());
}

#[test]
fn a_broken_held_data_sources_document_is_a_server_error_sharing_the_catalogues_code() {
    // `check_held`'s own refusal -- a duplicate id, or an entry that no
    // longer validates -- not an adapter's `DesiredStateError::Refused`,
    // which also covers a revoked credential and must not be told to an
    // operator the same way. Shares `InvalidDesiredState`/`InvalidCatalogue`'s
    // status and code rather than falling to the generic platform
    // mapping's `503`.
    let error = ControlPlaneError::Platform(PlatformError::InvalidHeldDataSources {
        detail: "shared-postgres-nz-01 is declared more than once".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.code(), "desired_state_invalid");
    assert!(error.public_message().contains("declared more than once"));
}

#[test]
fn an_adapter_refusal_falls_to_the_generic_platform_mapping_not_the_held_documents_code() {
    // `fabric-platform-git`'s port maps a revoked credential and a
    // rejected write to the same `DesiredStateError::Refused` `check_held`
    // uses for a broken document -- but `check_held`'s own failure now
    // arrives as `PlatformError::InvalidHeldDataSources`, so a bare
    // `Refused` reaching here is genuinely the adapter's, and must answer
    // the same retryable `503` hold and rollback get, not `500
    // desired_state_invalid` -- telling an operator whose GitHub App was
    // revoked that their data-sources file is broken would send them to
    // fix the wrong thing.
    let error =
        ControlPlaneError::Platform(PlatformError::DesiredState(PlatformDesiredStateError::Refused {
            detail: "the platform repository refused the platform's credential".to_owned(),
        }));

    assert_eq!(error.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.code(), "platform_unavailable");
}

#[test]
fn a_placement_refused_by_the_selector_and_a_logical_source_the_client_never_declared_share_one_code() {
    // ADR 0023 part 2: `select` refuses an intent no declared data source
    // admits, and a handler refuses a `{logical}` the client's own document
    // never named before `select` is ever asked -- different causes, and
    // deliberately the same answer, because to an operator both mean "this
    // cannot be placed" and the message beside the code says which.
    let refused_by_selector =
        ControlPlaneError::Platform(PlatformError::PlacementRefused(PlacementRefusal::AlreadyPlaced {
            tenant: tenant(),
            logical: logical(),
        }));
    let not_declared = ControlPlaneError::LogicalDataSourceNotDeclared { logical: logical() };

    assert_eq!(refused_by_selector.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(not_declared.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(refused_by_selector.code(), "placement_refused");
    assert_eq!(refused_by_selector.code(), not_declared.code());
    assert_ne!(
        refused_by_selector.public_message(),
        not_declared.public_message(),
        "the same code still carries a message that says which of the two happened"
    );
}

#[test]
fn a_broken_held_placements_document_is_a_server_error_sharing_the_data_sources_code() {
    // `placements::held::check_held_placements`'s own refusal -- a
    // duplicate (tenant, logical) pair, a data source nothing declares, an
    // isolation kind its data source does not serve, or two tenants with
    // one discriminator value. `InvalidHeldDataSources`'s sibling, and
    // shares its `500 desired_state_invalid` for the same reason: a
    // coherence problem in a document this platform itself authored, not
    // an outage upstream of it.
    let error = ControlPlaneError::Platform(PlatformError::InvalidHeldPlacements {
        detail: "acme is placed more than once for primary".to_owned(),
    });

    assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(error.code(), "desired_state_invalid");
    assert!(error.public_message().contains("placed more than once"));
}

#[test]
fn a_data_source_still_in_use_is_a_conflict_naming_every_tenant_placed_on_it() {
    // ADR 0023 part 2: removing a data source a placement still references
    // is refused, and the message names every tenant so an operator knows
    // who has to be unplaced first -- never a file, never an internal id
    // beyond the one they asked to remove.
    let error = ControlPlaneError::Platform(PlatformError::DataSourceInUse {
        id: data_source(),
        tenants: vec![
            TenantId::try_new("acme").unwrap(),
            TenantId::try_new("globex").unwrap(),
        ],
    });

    assert_eq!(error.status(), StatusCode::CONFLICT);
    assert_eq!(error.code(), "data_source_in_use");
    assert!(
        error.public_message().contains("acme"),
        "{}",
        error.public_message()
    );
    assert!(
        error.public_message().contains("globex"),
        "{}",
        error.public_message()
    );
}
