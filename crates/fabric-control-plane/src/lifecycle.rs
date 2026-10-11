//! A client's lifecycle as the control plane sees it (D01-14, D01-19a): the
//! evidence that decides whether saved intent is a usable client, the guard
//! every edit passes, and the tenants publication withholds.
//!
//! The states and their rules are `fabric_client_model::lifecycle`'s. This
//! module only gathers facts for them and applies them at the edges that
//! exist today: product, identity and secret edits, placement, and
//! publication. Routing has no producer in Fabric yet (the issuer registry
//! is D01-4); the rule that refuses it is in the domain, ready for one.

mod assess;
mod evidence;
#[cfg(test)]
mod evidence_tests;
mod guard;
mod withheld;

pub(crate) use guard::require;
pub(crate) use withheld::DesiredStateLifecycleSource;
