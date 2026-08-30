//! The host that assembles the Fabric authorization front.
//!
//! ```text
//! this container
//! ┌──────────────────────────────────────────────┐
//! │  fabric-fga-auth-api   :8080  (published)    │
//! │      registry → verifier → Check             │
//! │                    ↓                         │
//! │  openfga         127.0.0.1  (not published)  │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! The published address is the only way in. The authorization service is
//! started and supervised by this process, listens on loopback, and cannot be
//! addressed from outside the container — which is what makes running it with
//! no authentication of its own a containment rather than a hole (ADR 0016).

pub mod config;
pub mod embedded;
pub mod startup;
pub mod telemetry;
