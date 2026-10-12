//! The Data API's layer-2 adapter (ADR 0027).
//!
//! ```text
//! fabric-data-api            AuthorizationCheck port: may the caller <operation> <record | collection>?
//!       │
//!       ▼
//! fabric-fga-check           this crate: operation → computed relation (ADR 0013),
//!       │                    target → object, caller's own bearer → Authorization header
//!       ▼
//! authorization front        POST /v1/check {relation, object}       (ADR 0016)
//!       │                    verifies the token, binds the subject and the store
//!       ▼
//! OpenFGA                    127.0.0.1 only, never addressed from here
//! ```
//!
//! # Why the front and not OpenFGA
//!
//! ADR 0016 gives OpenFGA exactly one door, and it is the front. Speaking
//! OpenFGA's API from here would need the store, the model and the subject to
//! be supplied by the runtime — the three things the front exists to take from
//! a verified token instead. This adapter sends a relation and an object, and
//! the caller's own token. It has no credential of its own to send.
//!
//! # Verified TLS, and only a 200 is a decision
//!
//! A live [`FrontEndpoint`] is `https` and the client refuses anything else,
//! because the request carries the caller's token (ADR 0027 §2b). Plaintext
//! exists only on this crate's `cfg(test)` loopback path. A decision is read
//! only from an HTTP `200`.
//!
//! # Nothing constructs this in production
//!
//! OpenFGA is not deployed and no front endpoint is configured (D01-10a), so
//! the runtime host does not depend on this crate. Wiring it is a separately
//! reviewed change that names the endpoint.
//!
//! # Every failure refuses
//!
//! The port's rule, kept here: a transport failure or `5xx` is
//! [`CheckFailure::Unavailable`], any other answer that is not a decision is
//! [`CheckFailure::Internal`], and neither is ever turned into an allow or a
//! deny.
//!
//! # Records and collections (ADR 0027 §2a)
//!
//! `read`, `update` and `delete` are asked about a record, `customers:42`.
//! `list` and `create` are asked about the resource's collection,
//! `customers_collection:customers`, a distinct type the model writer
//! (`fabric-fga-model`) defines with only `can_list` and `can_create`. Both
//! spellings, and the `can_<operation>` relation, come from `fabric-core`, so
//! this adapter and the model writer cannot disagree about them. A question
//! whose operation and target disagree is refused before anything is sent: a
//! record is never asked a collection question, nor the reverse.
//!
//! [`CheckFailure::Unavailable`]: fabric_data_api::CheckFailure::Unavailable
//! [`CheckFailure::Internal`]: fabric_data_api::CheckFailure::Internal

mod front_check;
mod front_endpoint;
mod wire;

pub use front_check::FrontCheck;
pub use front_endpoint::FrontEndpoint;
