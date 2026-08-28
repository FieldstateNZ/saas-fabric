//! Checking what a write asked for against what the backend reported.
//!
//! # The defect this closes
//!
//! A write returns one number. The Data API used to copy it into the response
//! and answer `201 Created`, so a connector handed five rows that applied three
//! produced `{"affected":3}` under a created status: a success code for a
//! request that partly failed, and a count the caller had no way to read. The
//! count silently disagreeing with the request *is* the defect — not the
//! partial application itself, which the platform cannot prevent.
//!
//! # Why no capability could have prevented it
//!
//! The obvious repair is to consult
//! [`transactional_mutations`](fabric_connector::ConnectorCapabilities::transactional_mutations)
//! and refuse batches the backend cannot apply atomically. It fails twice.
//!
//! First, the flag does not mean that. It is negotiated from NDC's
//! `mutation.transactional` capability, which in the specification (v0.2.13)
//! governs the *cardinality of the `operations` array*: without it a caller
//! must send exactly one operation; with it a caller may send several and
//! expect them to succeed or fail together. A capability whose entire effect is
//! "you may now put more than one element in this array" says nothing about
//! what happens inside one element.
//!
//! Second, this platform puts every row of a batch into a single argument of a
//! single operation. NDC argument values are opaque JSON, so an argument
//! carrying one row and one carrying five hundred are indistinguishable to the
//! protocol. Atomicity of an N-row insert is the procedure's private business —
//! something the platform neither knows nor has any way to ask.
//!
//! # Why not simply refuse batches instead
//!
//! Because it would not close the defect. Nothing guarantees atomicity, so
//! "refuse unless guaranteed" means refusing every batch above one row
//! permanently — and a *single*-row insert still returns `affected: 0` when a
//! connector's procedure yields `null`, which under the old code was also
//! `201 Created`. The disagreement, not the batch size, is what has to be
//! caught, so it is caught at every size.
//!
//! # What remains unknowable
//!
//! Which rows landed. NDC's mutation response carries one opaque `result` per
//! operation and has no vocabulary for partial application: no per-row status,
//! no error variant, not even an affected count — the number checked here is
//! recovered heuristically by the connector from a procedure-defined shape.
//!
//! So the promise is narrow by construction. The platform knows how many rows
//! it sent and what number came back, and calls a write successful only when
//! those agree; when they do not it says so, and says plainly that it cannot
//! tell which rows applied. Rows a connector chose to return are still
//! projected into the response, so a backend implementing `RETURNING` lets the
//! caller find out — one that returns nothing leaves the caller to reconcile.

use fabric_connector::{ConnectorError, ConnectorId, MutationOutcome};

use crate::{DataApiError, OperationKind};

/// How many rows an operation is entitled to have affected.
pub(super) enum RowBudget {
    /// An insert of a batch the platform built itself, of exactly this many
    /// rows.
    ///
    /// Both a floor and a ceiling. Fewer means a partial application; more
    /// means the connector is describing something other than the request.
    Batch(u64),

    /// An update or delete addressing a single record by key.
    ///
    /// A ceiling only. Zero is a legitimate outcome — the key matched nothing —
    /// so a shortfall here is not a partial application and is not reported as
    /// one. There is no floor to check because the platform sends a predicate,
    /// not rows, and cannot know how many records should match.
    OneRecord,
}

impl RowBudget {
    /// The most rows this operation could honestly have affected.
    const fn ceiling(&self) -> u64 {
        match self {
            Self::Batch(rows) => *rows,
            Self::OneRecord => 1,
        }
    }
}

/// Rejects an outcome whose affected-row count disagrees with the write sent.
///
/// # Errors
///
/// - [`DataApiError::PartiallyApplied`] when fewer rows were applied than the
///   platform sent.
/// - [`ConnectorError::MalformedResponse`] when more rows were reported than
///   were sent. That is not a partial write but an incoherent answer: no
///   backend can affect six rows for a five-row insert, so the number describes
///   something other than the request the platform made — and once that is
///   true, nothing else the connector said about this operation can be relied
///   on either. The count is masked rather than published, because it would
///   tell a caller about rows that are not theirs.
///
/// `operation` is carried into the second of those because a `MalformedResponse`
/// is only ever built after a *success*, here included: the mutation ran. The
/// caller is told a 500 they cannot act on, but must not be told their write is
/// absent — see `errors::connector_mapping`.
pub(super) fn ensure_consistent(
    budget: &RowBudget,
    outcome: &MutationOutcome,
    connector: &ConnectorId,
    operation: OperationKind,
) -> Result<(), DataApiError> {
    let affected = outcome.affected_rows;
    let ceiling = budget.ceiling();

    if affected > ceiling {
        return Err(DataApiError::connector(
            ConnectorError::MalformedResponse {
                connector: connector.clone(),
                detail: format!("reported {affected} rows affected for a write of at most {ceiling}"),
            },
            operation,
        ));
    }

    match *budget {
        RowBudget::Batch(sent) if affected < sent => Err(DataApiError::PartiallyApplied {
            requested: sent,
            applied: affected,
        }),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connector() -> ConnectorId {
        ConnectorId::try_new("postgres").unwrap()
    }

    fn check(budget: &RowBudget, affected: u64) -> Result<(), DataApiError> {
        ensure_consistent(
            budget,
            &MutationOutcome::affected(affected),
            &connector(),
            OperationKind::Create,
        )
    }

    #[test]
    fn a_batch_that_fully_applied_is_consistent() {
        assert!(check(&RowBudget::Batch(5), 5).is_ok());
    }

    #[test]
    fn a_batch_that_partly_applied_is_reported_as_partial() {
        let error = check(&RowBudget::Batch(5), 3).unwrap_err();

        assert!(matches!(
            error,
            DataApiError::PartiallyApplied {
                requested: 5,
                applied: 3
            }
        ));
    }

    #[test]
    fn a_single_row_insert_that_applied_nothing_is_partial_too() {
        // The case that shows why refusing batches would not have sufficed.
        assert!(matches!(
            check(&RowBudget::Batch(1), 0).unwrap_err(),
            DataApiError::PartiallyApplied {
                requested: 1,
                applied: 0
            }
        ));
    }

    #[test]
    fn a_count_above_what_was_sent_is_a_malformed_response() {
        let error = check(&RowBudget::Batch(1), 500).unwrap_err();

        assert!(matches!(
            error,
            DataApiError::Connector {
                error: ConnectorError::MalformedResponse { .. },
                ..
            }
        ));
    }

    #[test]
    fn a_keyed_write_matching_nothing_is_consistent() {
        // Not a partial application: the platform sent a predicate, not rows.
        assert!(check(&RowBudget::OneRecord, 0).is_ok());
    }

    #[test]
    fn a_keyed_write_affecting_its_one_record_is_consistent() {
        assert!(check(&RowBudget::OneRecord, 1).is_ok());
    }

    #[test]
    fn a_keyed_write_claiming_several_records_is_a_malformed_response() {
        // A write addressed by key cannot honestly reach five rows.
        assert!(matches!(
            check(&RowBudget::OneRecord, 5).unwrap_err(),
            DataApiError::Connector {
                error: ConnectorError::MalformedResponse { .. },
                ..
            }
        ));
    }
}
