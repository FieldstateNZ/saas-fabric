//! A fake identity provider, for this crate's own tests and for
//! `fabric-control-plane`'s.
//!
//! # Why this is a fake and not a mock
//!
//! A mock would let a test assert that `create_realm_role` was called and stop
//! there. That is exactly the test that keeps passing after reconciliation
//! stops being idempotent, because a mock has no state for a second call to
//! observe.
//!
//! [`FakeIdentityProvider`] keeps state and answers `observe_realm` from it,
//! so every property this crate claims — a second pass changes nothing, a
//! missing role is created and its siblings are not, a provider failure leaves
//! desired state alone — is asserted against something that behaves like the
//! real thing. It also honours the port's idempotency contract: creating what
//! already exists succeeds.
//!
//! It is `pub` rather than `#[cfg(test)]` because `fabric-control-plane`'s own
//! `tests/` binaries build one directly, across the crate boundary — most
//! visibly the composed proof in `control_plane_api.rs` that drives a real
//! router through a whole reconciliation sweep
//! (`crates/fabric-control-plane/tests/support/mod.rs`,
//! `crates/fabric-control-plane/src/reconcile/pass_tests.rs`). It is **not**
//! wired in anywhere as a development adapter: an unconfigured deployment
//! gets no identity provider at all
//! (`IdentityProviderConfig::InMemory` in `fabric-control-plane-api`), and
//! every client simply reports `pending` forever rather than converging
//! against a fake nobody chose.

mod fake_identity_provider;
mod fake_provider_behaviour;

pub use fake_identity_provider::FakeIdentityProvider;
