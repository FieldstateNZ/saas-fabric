//! A thin driver over `std::process::Command` for the `docker` CLI.
//!
//! Deliberately not `testcontainers` (lead decision 2): the lifecycle this
//! harness needs is one network and a couple of containers, and this
//! workspace already hand-rolls its test-convenience layer rather than add
//! one (`TempDir` in `fabric-runtime-publication/tests/support/mod.rs`
//! instead of `tempfile`). Every function here that can fail returns a
//! [`DockerError`] carrying the exact command line and `stderr` -- nothing
//! is swallowed, because a container harness that fails silently just looks
//! like a flaky test to whoever hits it next.
//!
//! # Split into one file per concept
//!
//! This module was a single 422-line file. `docs/architecture/file-size-policy.md`'s
//! CI check does not itself reach `tests/`, but reviewers hold test support
//! to the same "one concept per file" convention it enforces for production
//! code, and 422 lines mixing several distinct concerns is well past what a
//! reviewer should have to hold in mind as "the docker module". It is split
//! along those concerns:
//!
//! - [`process`] -- driving the `docker` binary as a plain process: spawn
//!   it, read its exit status, turn failure into [`DockerError`]. Knows
//!   nothing about containers or networks.
//! - [`containers`] -- container lifecycle: start, inspect, exec into, stop,
//!   remove. Builds on `process`.
//! - [`image_reference`] -- what string a container start should actually
//!   pass to `docker run` for a digest-pinned image: presence checks,
//!   pulling, and the required-mode-versus-fallback policy. Builds on
//!   `process`; `containers::run` is its only caller.
//! - [`networks`] -- network lifecycle: create, remove, list by prefix.
//!   Builds on `process`, independently of `containers`.
//! - [`polling`] -- polling to a deadline. Knows about none of the above;
//!   every readiness check (`pg_isready`, a health-check binary, a repeated
//!   log line) is supplied by the caller.
//!
//! This file is the facade every other module in `support/` calls through --
//! `docker::run`, `docker::exec`, `docker::network_create`, and so on --
//! unchanged by the split: only the implementation moved, not the names any
//! of `connector.rs`, `postgres.rs`, `impostor.rs`, `names.rs`, `gate.rs` or
//! `stack.rs` call through.

mod containers;
mod image_reference;
#[cfg(test)]
mod image_reference_tests;
mod networks;
mod polling;
mod process;

pub use containers::{
    container_summaries_with_prefix, exec, exec_with_stdin, logs, port, rm, rm_by_name, run, stop, Container,
    RunSpec,
};
pub use networks::{network_create, network_rm, network_summaries_with_prefix};
pub use polling::poll_until;
pub use process::{ensure_success, version};

// `process::DockerError` is not re-exported here: nothing outside its own
// module names it by path today (every caller either constructs a `RunSpec`
// and lets `run` resolve the image, or inspects a `Result`'s `Err` through
// `.unwrap_or_else` without naming its type). Re-exporting an item nothing
// uses is exactly the unused import this facade should not be carrying --
// see `tests/support/mod.rs`'s removed blanket `unused_imports` allow. Add
// it back here the day a caller actually needs to name it.
//
// `image_reference` itself is not re-exported either: `containers::run`
// reaches `image_reference::resolve_runnable_reference` directly, and the
// module's only presence check, `image_present_at`, is called only from
// within `image_reference.rs` (a `pub fn image_present` wrapper around it
// once existed here too, with no caller anywhere in the crate, and was
// removed rather than kept "for symmetry").
