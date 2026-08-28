//! Reconciliation: making an identity provider match a client's desired state.
//!
//! ```text
//! desired Client              fabric-client-model
//!       ↓
//! IdentityReconciler          this crate — what should change, and did it
//!       ↓
//! IdentityProvider            this crate — the port, in platform vocabulary
//!       ↓
//! Keycloak adapter            fabric-keycloak — the protocol, and only there
//! ```
//!
//! # The split this crate exists to hold
//!
//! The reconciler owns **comparison and convergence semantics**: what the
//! desired state is, what the observed state is, which of the two differences
//! matter, in what order they are corrected, and what the result is called.
//! The adapter owns **one provider's protocol**. Neither knows the other's
//! job, and the seam between them is [`IdentityProvider`] — a trait written in
//! the platform's words (realm, role, application client), not Keycloak's
//! (`RealmRepresentation`, `clientScopes`, `attributes`).
//!
//! # Two properties worth stating plainly
//!
//! **Reconciliation is idempotent.** Running it three times leaves the same
//! provider state as running it once, and the second and third runs make no
//! calls that change anything. That is a property of this crate's diff, and it
//! is asserted rather than assumed — see `reconciler_tests`.
//!
//! **Reconciliation only adds.** Nothing here deletes a realm, a role, or an
//! application client. A role the document does not mention is left alone,
//! because a role that exists is a role something may already be granted, and
//! "the operator removed a line from a YAML file" is not enough evidence to
//! revoke it. Deletion is a separate decision with its own confirmation
//! path; ADR 0008 records that it is deliberately not made here.
//!
//! # A successful write is not a reconciled client
//!
//! The control plane writes desired state to Git; this crate makes a provider
//! match it. They are different events, they fail independently, and the gap
//! between them is visible rather than hidden — that is what
//! [`ReconciliationStatus`] is for.

#[cfg(test)]
mod fixtures;
mod logging;
mod plan;
mod provider;
mod reconciler;
mod status;

pub mod testing;

pub use plan::{IdentityAction, IdentityPlan};
pub use provider::{IdentityProvider, ObservedOidcClient, ObservedRealm, ProviderError};
pub use reconciler::{IdentityReconciler, ReconciliationOutcome};
pub use status::{ReconciliationReport, ReconciliationStatus, ReconciliationStatusStore};

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
///
/// The runtime plane's domains are 1–5. The control plane starts at 10, so a
/// number read out of a log line says which plane produced it before anything
/// else is looked up.
pub(crate) const DOMAIN_ID: u32 = 11;
