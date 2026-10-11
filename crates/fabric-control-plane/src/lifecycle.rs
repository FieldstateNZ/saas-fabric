//! A client's lifecycle as the control plane sees it (D01-14, D01-19a): the
//! evidence that decides whether saved intent is a usable client, the
//! evidence of whether a retired client is still published, and the guard
//! every edit passes.
//!
//! The states and their rules are `fabric_client_model::lifecycle`'s. This
//! module only gathers facts for them and applies them at the edges that
//! exist today: product edits, secret removal and placement. Publication is
//! not refused for a retired client: withdrawal is deferred (ADR 0021's
//! 2026-10-11 amendment), so its tenant stays published and the response
//! says so. Routing's producer is the issuer registry (D01-4): a client is
//! registered only while its lifecycle permits routing.

mod assess;
mod evidence;
#[cfg(test)]
mod evidence_tests;
pub(crate) mod facts;
mod guard;
mod withdrawal;
#[cfg(test)]
mod withdrawal_tests;

pub(crate) use assess::derive;
pub(crate) use guard::require;
pub(crate) use withdrawal::Withdrawal;
