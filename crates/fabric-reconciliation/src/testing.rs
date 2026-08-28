//! A fake identity provider, for tests and for running the control plane
//! without one.
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
//! It is `pub` rather than `#[cfg(test)]` because the control-plane host uses
//! it as a development adapter, so `cargo run` needs no Keycloak.

mod fake_identity_provider;
mod fake_provider_behaviour;

pub use fake_identity_provider::FakeIdentityProvider;
