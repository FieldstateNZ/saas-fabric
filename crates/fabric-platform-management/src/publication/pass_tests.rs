//! `outcome_from_publish_error`: which adapter refusals self-heal on the
//! next pass (`Failed`) and which need a human (`Refused`).

#![allow(clippy::unwrap_used)]

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::{DocumentKind, DocumentRevision, PublicationError};

use super::*;

fn unwritable() -> PublicationError {
    PublicationError::Unwritable {
        document: DocumentKind::Tenants,
        cause: Box::new(std::io::Error::other("disk full")),
    }
}

fn dangling_data_source() -> PublicationError {
    PublicationError::DanglingDataSource {
        tenant: TenantId::try_new("acme").unwrap(),
        logical: LogicalDataSourceName::try_new("primary").unwrap(),
        data_source: DataSourceId::try_new("shared-postgres-nz-01").unwrap(),
    }
}

#[test]
fn an_unwritable_target_self_heals_and_is_reported_as_failed() {
    // The cluster or the disk can be half-updated by the write that just
    // failed, so this is a transport problem to retry, not a document to
    // refuse.
    let outcome = outcome_from_publish_error(&unwritable());

    assert!(matches!(outcome, PassOutcome::Failed { .. }), "{outcome:?}");
}

#[test]
fn an_unreadable_held_document_is_also_reported_as_failed() {
    let error = PublicationError::Unreadable {
        document: DocumentKind::Catalog,
        cause: Box::new(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        )),
    };

    let outcome = outcome_from_publish_error(&error);

    assert!(matches!(outcome, PassOutcome::Failed { .. }), "{outcome:?}");
}

#[test]
fn a_stale_revision_self_heals_on_the_next_read_and_is_reported_as_failed() {
    let error = PublicationError::StaleRevision {
        document: DocumentKind::DataSources,
        held: DocumentRevision::new(4),
        offered: DocumentRevision::new(3),
    };

    let outcome = outcome_from_publish_error(&error);

    assert!(matches!(outcome, PassOutcome::Failed { .. }), "{outcome:?}");
}

#[test]
fn a_dangling_data_source_is_a_document_a_human_must_fix_and_is_refused() {
    // Nothing about retrying fixes a tenant binding that names a data
    // source this same publication does not include.
    let outcome = outcome_from_publish_error(&dangling_data_source());

    assert!(matches!(outcome, PassOutcome::Refused { .. }), "{outcome:?}");
}

#[test]
fn an_exhausted_divergent_payload_budget_is_refused_not_retried_again() {
    let error = PublicationError::DivergentPayload {
        document: DocumentKind::Tenants,
        revision: DocumentRevision::new(2),
    };

    let outcome = outcome_from_publish_error(&error);

    assert!(matches!(outcome, PassOutcome::Refused { .. }), "{outcome:?}");
}
