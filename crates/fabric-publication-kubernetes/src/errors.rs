//! The one error shape this adapter adds: a transport failure, in words that
//! carry no address and no credential.

use fabric_runtime_publication::{DocumentKind, PublicationError};

/// What the cluster did, said in the adapter's own words.
///
/// Never the response body, never a URL: the cluster's answer may name
/// things an operator's console must not see, and the request carried a
/// bearer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub(crate) struct Transport(pub(crate) String);

pub(crate) fn unreadable(document: DocumentKind, detail: impl Into<String>) -> PublicationError {
    PublicationError::Unreadable {
        document,
        cause: Box::new(Transport(detail.into())),
    }
}

pub(crate) fn unwritable(document: DocumentKind, detail: impl Into<String>) -> PublicationError {
    PublicationError::Unwritable {
        document,
        cause: Box::new(Transport(detail.into())),
    }
}
