//! Names the authorization model reserves.
//!
//! The model writer (`fabric-fga-model`) emits these, and the client model's
//! authorization validation keeps declared names clear of them. They are here
//! so the two cannot disagree.

/// The one principal type every model has. The front binds every subject as
/// `user:<realm>/<subject>` (ADR 0015), so a declared resource cannot also be
/// called `user`.
pub const PRINCIPAL_TYPE: &str = "user";

/// Names the authorization service reserves for itself, as a type or as a
/// relation.
pub const SERVICE_RESERVED: [&str; 2] = ["self", "this"];
