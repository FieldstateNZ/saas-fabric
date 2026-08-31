//! Reading published artifacts from an OCI registry.
//!
//! ```text
//! Registry              the port, owned by fabric-platform-management
//!      ↑
//! OciRegistry           ← the translation happens here, and only here
//!      ↓
//! /v2/<name>/tags/list, /manifests/<ref>, /blobs/<digest>
//! ```
//!
//! # Anonymous, and deliberately so
//!
//! The SaaS Fabric packages are public, so this holds no credential at all —
//! it exchanges an anonymous pull token per repository and reads. That is one
//! fewer secret on the path between a published preview and an environment,
//! and it keeps a boundary clean by construction: **the GitHub App that writes
//! platform desired state is not, and must never become, the registry
//! credential.** They are separate integrations, and a credential that does
//! not exist cannot be conflated with another one.
//!
//! When a package eventually needs authenticating to, that is a registry
//! integration with its own configuration — not a wider scope on an existing
//! App.
//!
//! # Nothing is remembered between passes
//!
//! There is no cache of what was found, and that is a correctness property
//! rather than a simplification. A component's images are published by
//! parallel jobs, so a version present in two repositories and not the third
//! is an ordinary minutes-long window; an adapter that remembered "not there"
//! would still believe it an hour later.
//!
//! The one thing held between calls is the pull token, which is a credential
//! and not an answer.

mod client;
mod errors;

pub use client::OciRegistry;
