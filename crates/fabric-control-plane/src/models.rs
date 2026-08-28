//! The shapes the control-plane API speaks.
//!
//! # Domain concepts, never repository internals
//!
//! Every field here is something an operator recognises: a client, a realm, a
//! role, an application client, a reconciliation status. None of them is a
//! file, a path, a line number, a branch, or a commit (specification §8). The
//! one field that comes from the repository — `revision` — is opaque by
//! construction and is used the way an HTTP entity tag is used, which is the
//! whole of what an operator needs to know about it.
//!
//! # Why the request type reuses the domain's application-client type
//!
//! Because the API's contract for an application client *is* the document's:
//! the operator is editing desired state, and inventing a parallel shape would
//! mean two definitions of the same thing that could disagree. The response
//! types are separate because they carry something the document does not —
//! reconciliation status — and that difference is real.

mod client_response;
mod identity_request;
mod identity_response;
mod reconciliation_response;

pub(crate) use client_response::{ClientListResponse, ClientResponse};
pub(crate) use identity_request::IdentityRequest;
pub(crate) use identity_response::IdentityResponse;
pub(crate) use reconciliation_response::ReconciliationResponse;
