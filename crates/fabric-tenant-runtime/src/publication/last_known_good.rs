//! The reader's persisted last-known-good generation (gap G5).
//!
//! A running reader holds its last complete generation in memory, which a
//! restart loses. A replica restarting while the published volumes are torn
//! or projected one ahead of another then had nothing to prime from and
//! failed to start. With a writable directory configured
//! (`RuntimeConfig::last_known_good_dir`), the reader also keeps that
//! generation on disk: the record and the exact tenants, data-sources and
//! catalogue bytes it names, replaced atomically each time a newer
//! published generation validates.
//!
//! # When the copy is served, and when the reader fails closed
//!
//! | At start | Published files | Valid copy | Applied |
//! |---|---|---|---|
//! | | complete generation N | none, or M ≤ N | N, and persisted |
//! | | complete generation N | M > N | M (a generation never goes back) |
//! | | torn, mismatched, unreadable, above this reader, or no record | M | M |
//! | | torn, mismatched, unreadable or above this reader | none | nothing: the prime fails |
//!
//! Once running, a refresh applies a published generation at or above the
//! highest one this replica served or persisted, and holds against anything
//! lower, or a missing record ([`SourceError::Regressed`](crate::SourceError)).
//! A copy is refused, and never served, if its record or any document does
//! not match its digest, if its reader floor is above this reader, or if a
//! document does not parse. The reader never repairs, rewrites or deletes a
//! refused copy, and never writes anything but a published generation that
//! validated; only an operator discards one (`fabric-api last-known-good`).

mod atomic_replace;
#[cfg(test)]
mod atomic_replace_tests;
mod choice;
#[cfg(test)]
mod choice_tests;
mod discard;
mod envelope;
#[cfg(test)]
mod fixtures;
mod holder;
mod inspection;
mod refusal;
mod state;
mod store;
#[cfg(test)]
mod store_tests;

pub(crate) use choice::Choice;
pub use discard::{DiscardRequest, Discarded, AUDIT_FILE_NAME};
pub(crate) use holder::LastKnownGood;
pub use inspection::{Inspection, Verdict};
pub use refusal::LastKnownGoodRefusal;
pub use state::LastKnownGoodState;
pub use store::LastKnownGoodStore;
