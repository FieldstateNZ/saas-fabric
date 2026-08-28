//! Host-level pieces of the SaaS Fabric control plane.
//!
//! # A second binary, not a second API on the first one
//!
//! The runtime plane's host (`fabric-api`) and this one are separate
//! processes, separate images, and separate deployments. That is not tidiness:
//!
//! - They face different networks. The runtime API is on the product edge,
//!   reachable by every tenant's application. This one is on the **operator**
//!   plane and must not be reachable from the product edge at all.
//! - They fail differently. A control plane that cannot reach Git is broken;
//!   a runtime plane in the same situation has not noticed, because it never
//!   reads Git (specification §6). Sharing a process would couple their
//!   availability, which is the exact coupling the architecture forbids.
//! - They authenticate different things — a tenant, and a platform operator
//!   (ADR 0009).
//!
//! # Deliberately thin
//!
//! The binary loads configuration, builds the application, serves it, and
//! stops. Everything worth testing lives here in the library half, where an
//! integration test can reach it — including the check that the example
//! configuration shipped in `examples/` actually loads, so it cannot drift
//! into something that is trusted and wrong.

pub mod config;
pub mod secrets;
pub mod startup;
pub mod telemetry;
