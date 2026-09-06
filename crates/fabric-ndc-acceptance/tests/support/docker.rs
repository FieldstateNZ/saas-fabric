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
//! code, and 422 lines mixing four distinct concerns is well past what a
//! reviewer should have to hold in mind as "the docker module". It is split
//! along those four concerns:
//!
//! - [`process`] -- driving the `docker` binary as a plain process: spawn
//!   it, read its exit status, turn failure into [`DockerError`]. Knows
//!   nothing about containers or networks.
//! - [`containers`] -- container lifecycle: start, inspect, exec into, stop,
//!   remove. Builds on `process`.
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
mod networks;
mod polling;
mod process;

pub use containers::{
    container_names_with_prefix, exec, exec_with_stdin, image_present, logs, port, rm, rm_by_name, run, stop,
    Container, RunSpec,
};
pub use networks::{network_create, network_names_with_prefix, network_rm};
pub use polling::poll_until;
pub use process::{ensure_success, version, DockerError};
