//! The protected continuity state for client lifecycles, and the admission
//! guard every identity side effect passes (D01-14 restore; O5, #108
//! comments 6119161317 and 6120741272).
//!
//! # Partial: no durable adapter exists
//!
//! A tombstone cannot be undone by Git, and nothing Git holds can prove a
//! lifecycle is the one an operator last wrote: a revert, a forged or erased
//! `restorations` list and a blob-SHA ABA all read as ordinary documents. The
//! authority is a protected store, independent of editable desired state,
//! holding each client's identity instance, its continuity sequence and the
//! fingerprint of the lifecycle it last accepted, with reservation and
//! check-and-set semantics ([`LifecycleContinuity`]).
//!
//! That store does not exist yet. A deployment is built with none, and then:
//!
//! - every restore is held (`503 lifecycle_continuity_unavailable`);
//! - no client is admitted ([`admit`]), so identity readiness is unmet, no
//!   pass creates, updates or re-asserts a provider client, and a
//!   publication pass whose issuer registry would need an admission holds
//!   rather than publishing a registry decided without one;
//! - a creation, tombstone or deletion confirmation still writes desired
//!   state: the first stages intent, the others only restrict.
//!
//! Holds change nothing that exists: provider clients, callbacks, secrets,
//! bindings, accepted publication artifacts and last-good readers stay as
//! they are. `testing::InMemoryLifecycleContinuity` is test-only and proves
//! the classification, never restart, crash or concurrency safety.

mod admission;
mod fingerprint;
mod hold;
mod port;
mod refusal;
mod reserve;

pub(crate) use admission::{admit, admit_all, Admission};
pub use fingerprint::LifecycleFingerprint;
pub use hold::ContinuityHold;
pub use port::{AcceptedLifecycle, LifecycleContinuity, PendingTransition, Transition, TransitionKind};
pub use refusal::{ContinuityRefusal, ContinuityReservation};
pub(crate) use reserve::{mint, reserve, Reserving};
