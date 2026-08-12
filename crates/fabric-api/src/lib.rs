//! Host-level pieces of the SaaS Fabric runtime plane.
//!
//! The binary in `main.rs` is the composition root and stays deliberately thin.
//! Everything it needs that is worth testing — configuration loading and
//! validation, the health probes, the secret resolver — lives here, where an
//! integration test can reach it.
//!
//! That split exists for one concrete reason: the example configuration shipped
//! in `examples/` should be *known* to load, not merely believed to. A config
//! file that has drifted from the code is worse than no example, because it is
//! trusted.

pub mod config;
pub mod health;
pub mod secrets;
pub mod telemetry;
