//! Assembling the control plane.

mod adapters;
mod application;
mod health;
mod integration;
mod operator_keys;
mod platform;
mod platform_target;
mod registries;
mod reserved_names;
mod serving;
mod shutdown;
mod tick;

pub use application::{build, Application};
pub use registries::{compose_registries, ComposedRegistries, RegistryComposition};
pub use shutdown::shutdown_signal;
