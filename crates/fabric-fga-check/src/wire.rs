//! The front's `POST /v1/check` shapes, and nothing else of its API.

use serde::{Deserialize, Serialize};

/// The request body. The front's `CheckRequest` refuses unknown fields, and
/// has no `user`, `store` or `tenant` to send — the token decides all three.
#[derive(Debug, Serialize)]
pub(crate) struct CheckBody<'a> {
    /// The computed relation, `can_<operation>`.
    pub(crate) relation: &'a str,

    /// `resource:id` for a record, `resource_collection:resource` for a
    /// collection.
    pub(crate) object: &'a str,
}

/// The only part of the answer this crate reads. A denial is
/// `200 {"allowed": false}`, never `403` (ADR 0016).
#[derive(Debug, Deserialize)]
pub(crate) struct CheckAnswer {
    /// Whether the relation holds.
    pub(crate) allowed: bool,
}
