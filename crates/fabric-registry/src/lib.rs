//! Reading published artifacts from OCI registries.
//!
//! ```text
//! Registry              the port, owned by fabric-platform-management
//!      ↑
//! Registries            ← by the repository's host, never a default
//!      ↓
//! OciRegistry           ← the translation happens here, and only here
//!      ↓
//! /v2/<name>/tags/list, /manifests/<ref>, /blobs/<digest>, /referrers/<digest>
//! ```
//!
//! # A credential where it was registered, and nowhere else
//!
//! Reading needs none: the SaaS Fabric packages are public, and a repository
//! nobody registered a credential for is read anonymously. When an operator
//! gives a registry one (ADR 0026 section 5) it is held as a
//! [`RegistrySecret`] nothing can print, and presented only for the
//! repositories registered under that registry, only to the realm its
//! kind's [`RealmRule`] allows — a challenge naming another is refused before
//! a byte goes to it — and never across origins. Nothing is presented before
//! a challenge asks for it. A credential its realm refuses is marked, and not
//! presented again by any client sharing its mark
//! ([`Credential::sharing_refusal`]); replacing it is building a new
//! [`OciRegistry`] with a fresh one, so no token or refusal outlives it.
//!
//! **The GitHub App that writes platform desired state is not, and must
//! never become, the registry credential.** They are separate integrations.
//!
//! # Public addresses, for a registry an operator registered
//!
//! Under [`AddressPolicy::PublicOnly`] every name is resolved, on every
//! connection, to its public addresses alone, and no URL naming an IP literal
//! is followed — so a registry, its realm or its CDN cannot point Fabric at
//! a metadata endpoint or an internal network. The deployment's own
//! registry, which its configuration places, is [`AddressPolicy::Any`].
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
//! an answer, a credential's refusal, which is about the credential and not
//! the registry, and a bounded cache of bytes already verified by digest, which
//! is content and not an answer either: what a digest names cannot change.
//!
//! # HTTPS, bounded, and on one origin
//!
//! Every request is HTTPS, on every hop; plain HTTP to loopback exists only in
//! a constructor tests use. Manifests, tags, referrers and tokens follow a
//! redirect only to the origin they were sent to, and a `Link` only on the
//! registry's own; a blob follows one to any HTTPS origin, because every
//! hosted registry serves blobs from a CDN. Those hops are followed here,
//! one by one, and the pull token or credential goes only to a hop on the
//! registry's own origin, however long the chain. No request carries a
//! `Referer` or uses an ambient proxy, and every body is read within a bound
//! before it is parsed (ADR 0026 section 4).
//!
//! # A component descriptor is found here and read elsewhere
//!
//! [`Registry::component_descriptor`](fabric_platform_management::Registry::component_descriptor)
//! reads the referrers API and the referrers tag schema, checks every
//! candidate by digest, and hands back the one document's bytes. It never
//! parses them: that is `fabric-component`'s, in the domain.

mod address;
mod charts;
mod client;
mod errors;
mod registries;
mod settings;
mod transport;

pub use charts::HelmCharts;
pub use client::{OciRegistry, Proof, Readability};
pub use registries::Registries;
pub use settings::{AddressPolicy, Credential, RealmRule, RegistrySecret, RegistrySettings};
