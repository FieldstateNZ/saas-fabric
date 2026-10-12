//! Read-only deployment evidence, isolated from both the runtime and Git writers.
mod client;
mod config;
mod evaluate;
mod image;
mod observer;
mod reader;
mod summary;
mod wire;

pub use config::WorkloadTarget;
pub use observer::KubernetesObserver;
pub use reader::{KubernetesReaderObserver, ReaderTarget};
