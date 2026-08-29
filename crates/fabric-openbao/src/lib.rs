//! Where a Fabric instance's secrets and integration record actually live.
//!
//! # Why the application talks to OpenBao at all
//!
//! Until now it did not have to. Every credential was created by a human and
//! delivered into the pod's environment by the External Secrets Operator, and
//! reading the environment was enough.
//!
//! That is a one-way path. The moment the platform started *generating*
//! credential material — a GitHub App's private key, which the host returns
//! exactly once — it needed somewhere to put it, and no amount of projecting
//! secrets inward provides that. So the control plane becomes a client of the
//! secret store rather than only a reader of what somebody projected.
//!
//! # What this crate is, and what it is not
//!
//! It implements two ports the control plane owns, and it is the only place in
//! this workspace that knows OpenBao exists. Nothing above it names a mount, a
//! path, an authentication method or a lease — the domain asks for a secret by
//! name within an instance's partition, and where that lands is decided here.

mod auth;
mod client;
mod config;
mod integration_store;
mod kv;
mod secret_store;

pub use client::OpenBao;
pub use config::OpenBaoConfig;
pub use integration_store::OpenBaoIntegrationStore;
pub use secret_store::OpenBaoSecretStore;
