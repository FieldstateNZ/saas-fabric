//! A Git host that answers the Git Data API over a real socket.
//!
//! # Why a stateful fake and not a stub
//!
//! The property under test is a *race*, and a race is a property of a
//! sequence: read the head, somebody else commits, try to move the branch,
//! refused. A stub returning a canned `409` would prove the error mapping and
//! nothing about whether the adapter builds its commit on the head it read,
//! rebuilds on the new one, or notices that the file it is writing moved.
//!
//! [`FakePlatformHost`] keeps blobs, trees, commits and a branch, applies a
//! commit only when its parent is the current head, and keeps a snapshot per
//! commit so a read pinned to a revision answers what that revision actually
//! held. It also refuses `force: true` outright — the adapter must never send
//! it, and a fake that quietly accepted it would let that regression pass.

// Each test binary compiles the whole support module but uses a subset.
#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fake_platform_host;
mod http_server;

pub use fake_platform_host::{FakePlatformHost, BRANCH, OWNER, REPOSITORY};
