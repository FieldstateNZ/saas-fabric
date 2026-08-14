//! The connector failures a test cannot simply construct inline.
//!
//! Factories rather than values because [`ConnectorError`] is not `Clone`: its
//! transport variants box an arbitrary source, so a suite driving several
//! requests needs a fresh one each time.
//!
//! Every transport source below names a host and a port on purpose. The status
//! these failures now answer is more informative than the one they used to, so
//! each assertion about a status doubles as a check that the *message* did not
//! become more informative too (§2, §29).

use fabric_connector::{ConnectorError, ConnectorId};

fn connector() -> ConnectorId {
    ConnectorId::try_new("postgres").unwrap()
}

/// A source whose text names infrastructure no caller may read.
fn revealing_source() -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::other(
        "sql-au-east-03.internal:5432 dropped the connection",
    ))
}

/// Provably not delivered: a refused connect, a DNS failure, an unbuildable
/// request. The only transport failure a write may safely repeat.
pub fn unreachable() -> ConnectorError {
    ConnectorError::Unreachable {
        connector: connector(),
        source: revealing_source(),
    }
}

/// The request went out and no answer came back. It may or may not have been
/// carried out.
pub fn outcome_unknown() -> ConnectorError {
    ConnectorError::OutcomeUnknown {
        connector: connector(),
        source: revealing_source(),
    }
}

/// A success status was read off the wire and the body then died. The write
/// took effect; only its result is gone.
pub fn result_lost() -> ConnectorError {
    ConnectorError::ResultLost {
        connector: connector(),
        source: revealing_source(),
    }
}

/// The backend refused the operation, carrying text that names a schema.
pub fn rejected() -> ConnectorError {
    ConnectorError::Rejected {
        connector: connector(),
        message: "duplicate key value violates constraint on acme_prod.customers".to_owned(),
    }
}

/// The backend answered successfully in a shape that could not be read.
pub fn malformed_response() -> ConnectorError {
    ConnectorError::MalformedResponse {
        connector: connector(),
        detail: "expected an object".to_owned(),
    }
}
