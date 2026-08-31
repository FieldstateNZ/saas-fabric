//! An OCI registry answering over a real socket.
//!
//! # Why a fake registry and not a fake `Registry`
//!
//! `fabric-platform-management` already tests the *rules* against a trait
//! implemented in three lines. What that cannot check is whether this adapter
//! reads a registry correctly: whether a `404` becomes `None` rather than an
//! error, whether the digest pinned is the tag's own and not a platform's,
//! whether pagination is followed, whether a `401` mints a new token.
//!
//! Every one of those is a property of the wire, so the fake is on the wire.

#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fake_registry;
mod http_server;

pub use fake_registry::{FakeRegistry, HOST};
