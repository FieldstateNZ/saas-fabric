//! The Git-backed desired-state repository.
//!
//! ```text
//! ClientService               client, identity, revision
//!       ↓
//! ClientRepository            the port — still SaaS Fabric's words
//!       ↓
//! GitClientRepository         ← the translation happens here, and only here
//!       ↓
//! Git hosting contents API    paths, blobs, commits, branches
//! ```
//!
//! # Optimistic concurrency, not last-writer-wins
//!
//! A revision is the stored file's blob hash, and a write carries the hash the
//! caller believed it was editing. The hosting API applies the write only if
//! that hash is still current, and answers `409` otherwise — so the check is
//! **atomic on the server**, not a read-then-write in this process that a
//! second control-plane replica could interleave with. ADR 0008 is explicit
//! that a concurrent edit must be refused rather than merged or overwritten.
//!
//! # No Git library, deliberately
//!
//! This crate speaks the hosting provider's contents API over HTTPS. It does
//! not link `git2`, `gix`, or anything else that would put a Git
//! implementation in the workspace's dependency graph —
//! `scripts/check_architecture.py` fails the build if one appears, and that
//! check is what keeps "Git is never in the request path" a structural fact
//! about the whole binary rather than a claim about the runtime crates.
//!
//! It also means the platform needs no working copy, no clone, and no disk: a
//! control-plane replica is stateless, and two of them behave the same way
//! because neither has a local view to diverge.
//!
//! # What this crate does not decide
//!
//! What a client should look like, whether an edit is legal, who asked for it,
//! or what happens next. It writes a document at a revision and reports what
//! happened.

mod config;
mod credential;
mod factory;
mod github;
mod logging;
mod provisioning;
mod repository;

pub use config::{GitAuthConfig, GitRepositoryConfig};
pub use credential::GitCredential;
pub use factory::GitRepositoryFactory;
pub use provisioning::GitHubAppProvisioning;
pub use repository::GitClientRepository;

/// The event-ID domain number for this crate. See `fabric_core::event_id`.
pub(crate) const DOMAIN_ID: u32 = 13;
