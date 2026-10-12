//! Deciding whether an established identity may perform an operation.
//!
//! Specification §23 separates two questions that are easy to conflate:
//!
//! - **Tenant resolution** — which tenant's resources does this target?
//! - **Authorization** — may this identity perform this operation?
//!
//! This module answers only the second. Nothing here can influence the first:
//! every decision receives the tenant the identity already carries and can
//! only answer yes or no about it.
//!
//! # Two layers (ADR 0027)
//!
//! ```text
//! layer 1  scopes             data:<resource>:<read|write> on the token      every request
//!             │ refused → 403, layer 2 is never asked
//!             ▼
//! layer 2  relation Check     the authorization front's Check (ADR 0016)     only where the
//!             │                                                              tenant's policy
//!             │ denied → 403; unreachable or broken → refused (fail closed)  requires it
//!             ▼
//!          permitted
//! ```
//!
//! Layer 2 is required when the tenant is *not* the isolated synthetic trial
//! and the resource declares relations. That choice is per tenant, in
//! [`TenantPolicies`], never a deployment-wide switch.
//!
//! # What is deliberately absent (D01-12)
//!
//! - **No administrator role.** A role from a tenant realm never bypasses a
//!   data-path check. Coarse operator controls (health detail) live in the
//!   host, outside this crate, and grant nothing here.
//! - **No way to switch scopes off.** There is no configuration field, flag or
//!   constructor that skips layer 1, in test builds or otherwise.

mod caller;
mod check;
mod check_object;
mod check_target;
mod data_authorization;
mod no_check_configured;
mod refusal;
mod required_scope;
mod tenant_authorization;
mod tenant_policies;

/// The operation vocabulary, which lives in the crate both planes share.
///
/// Re-exported rather than redefined: a client's desired state declares which
/// relations permit which operations, so the control plane names these too,
/// and two definitions of "read" would drift without ever failing a build.
pub use fabric_core::OperationKind;

/// A resource's collection, spelled in the crate both planes share for the
/// same reason: the control plane writes its type into the model and this
/// crate asks about it.
pub use fabric_core::CollectionObject;

pub use caller::{Caller, CallerCredential};
pub use check::{AuthorizationCheck, CheckDecision, CheckFailure, CheckQuestion};
pub use check_object::CheckObject;
pub use check_target::CheckTarget;
pub use data_authorization::DataAuthorization;
pub use no_check_configured::NoCheckConfigured;
pub(crate) use refusal::{DenialReason, Refusal};
pub use required_scope::required_scope;
pub use tenant_authorization::{TenantAuthorization, TenantPosture};
pub use tenant_policies::TenantPolicies;
