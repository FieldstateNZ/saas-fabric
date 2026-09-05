//! The filesystem adapter: three payload files and their sidecar manifests,
//! written atomically, in the order ADR 0018 part 3 requires.
//!
//! # Why `std::fs`, not `tokio::fs`
//!
//! This crate's `tokio` workspace dependency is not declared with the `fs`
//! feature — only `macros`, `rt-multi-thread`, `sync`, and `time`, which is
//! what the runtime plane's own async work needs. Adding `fs` here to avoid
//! a blocking call would mean growing that feature set workspace-wide for
//! one adapter that a control-plane scheduler calls, at most, on a poll
//! interval — not a request-path hot loop. So [`FilesystemRuntimePublication`]
//! does synchronous `std::fs` I/O inside its `async fn`s. That is a
//! deliberate trade, not an oversight: a future caller running this inside a
//! busy async executor should wrap it in `spawn_blocking` if it ever
//! contends with other work on the same runtime.

mod adapter;
mod atomic_write;
mod held;
mod parse;
mod paths;
mod plan;
mod write;

pub use adapter::FilesystemRuntimePublication;
