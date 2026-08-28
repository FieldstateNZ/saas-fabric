//! A Git host that answers over a real socket.
//!
//! # Why a stateful fake and not a stub
//!
//! The property that matters most here is optimistic concurrency, and it is a
//! property of a *sequence*: read, someone else writes, write, refused. A stub
//! that returned a canned `409` would pass a test of the error mapping and
//! prove nothing about whether the adapter sends the blob hash at all.
//!
//! [`FakeGitHost`] keeps files, moves their hashes on every accepted write, and
//! refuses a write whose hash is stale — which is exactly what the real
//! contents API does, and the only thing that makes "a stale revision is
//! refused" a real assertion.

// Each test binary compiles the whole support module but uses a subset.
#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod fake_git_host;
mod http_server;

pub use fake_git_host::{FakeGitHost, MINTED_TOKEN};
