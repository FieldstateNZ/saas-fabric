//! Reading published artifacts from an OCI registry.
//!
//! ```text
//! Registry              the port, owned by fabric-platform-management
//!      ↑
//! OciRegistry           ← the translation happens here, and only here
//!      ↓
//! /v2/<name>/tags/list, /manifests/<ref>, /blobs/<digest>, /referrers/<digest>
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
//! # Every digest is one it computed
//!
//! Every manifest and blob this reads is hashed with SHA-256, and a digest it
//! reports is the hash of bytes it read — never a `Docker-Content-Digest`
//! header, which is a pointer it checks. A tag is resolved by `HEAD`, then
//! the bytes of the digest the registry names, and a mismatch anywhere is
//! refused. Only `sha256` is accepted (ADR 0026 section 3).
//!
//! # Nothing is remembered between passes, except content
//!
//! There is no cache of what was *found*, and that is a correctness property
//! rather than a simplification. A component's images are published by
//! parallel jobs, so a version present in two repositories and not the third
//! is an ordinary minutes-long window; an adapter that remembered "not there"
//! would still believe it an hour later. A `404`, a tag's current digest, a
//! referrers list: every one is asked again.
//!
//! What is held between calls is the pull token, which is a credential and not
//! an answer, and a bounded cache of bytes already verified by digest, which
//! is content and not an answer either: what a digest names cannot change.
//!
//! # HTTPS, bounded, and on one origin
//!
//! Every request is HTTPS, on every hop; plain HTTP to loopback exists only in
//! a constructor tests use. Manifests, tags, referrers and tokens follow a
//! redirect only to the origin they were sent to, and a `Link` only on the
//! registry's own; a blob follows one to any HTTPS origin, because every
//! hosted registry serves blobs from a CDN. Those hops are followed here,
//! one by one, and the pull token goes only to a hop on the registry's own
//! origin, however long the chain. No request carries a `Referer` or uses an ambient proxy, and every
//! body is read within a bound before it is parsed (ADR 0026 section 4).
//!
//! # A component descriptor is found here and read elsewhere
//!
//! [`Registry::component_descriptor`](fabric_platform_management::Registry::component_descriptor)
//! reads the referrers API and the referrers tag schema, checks every
//! candidate by digest, and hands back the one document's bytes. It never
//! parses them: that is `fabric-component`'s, in the domain.

mod charts;
mod client;
mod errors;
mod transport;

pub use charts::HelmCharts;
pub use client::OciRegistry;
