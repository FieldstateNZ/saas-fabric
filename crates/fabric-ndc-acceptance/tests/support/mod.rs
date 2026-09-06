//! The container harness: docker-driven test support for issue #62.
//!
//! Deliberately not shared with
//! `crates/fabric-runtime-publication/tests/support/`, even though the
//! shapes rhyme on purpose (a `TempDir`-equivalent naming scheme, a `Stack`
//! with `Drop` teardown, deadline-bounded polling instead of a sleep
//! deciding an outcome). That module is `tests/`-local to a crate this one
//! must not depend on -- see `crates/fabric-ndc-acceptance/src/lib.rs` for
//! the three architecture checks that make `fabric-ndc-acceptance` the only
//! place a test needing both `fabric-runtime-publication` and
//! `fabric-connector-ndc` can live. The corpus also has to diverge: this
//! one is real SQL seeded by literal (`postgres.rs`), never the Rust
//! fixture constants `RecordingConnector` uses, because a mutation to a
//! published binding must not be able to move the corpus with it
//! (`docs/verification.md` row 1a).
//!
//! Every test in this crate's `tests/` calls [`gate::docker_available_or_skip`]
//! first and returns immediately if it answers `false`.

#![allow(
    dead_code,
    unused_imports,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

pub mod compose;
pub mod connector;
pub mod docker;
pub mod fixtures;
pub mod gate;
pub mod images;
pub mod impostor;
pub mod names;
pub mod postgres;
pub mod requests;
pub mod stack;
pub mod tempdir;
pub mod unsigned_reader;
