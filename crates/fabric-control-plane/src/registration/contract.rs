//! What the host hands in, and what it gets back: two structs, kept apart
//! from the platform binding they both name because each is genuinely one
//! concept — `deps.rs` is `ControlPlaneDeps`, `services.rs` is
//! `ControlPlaneServices`.

mod deps;
mod services;

pub use deps::ControlPlaneDeps;
pub use services::ControlPlaneServices;
