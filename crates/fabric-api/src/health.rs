//! Liveness and readiness.
//!
//! # Why these are two different questions
//!
//! **Liveness** asks whether the process is wedged. It must never depend on
//! anything external — a liveness probe that fails when a database is down
//! causes the orchestrator to restart every replica during an outage, turning
//! a degraded system into a dead one.
//!
//! **Readiness** asks whether this replica can serve traffic. It genuinely
//! should fail when a registry has not primed, because a replica with no
//! bindings or no DataSources can serve no tenant and would 503 everything it
//! was sent (§28). It should **not** fail merely because one of several
//! connectors is unhealthy — see the `readiness_state` submodule for that
//! decision and why partial connector failure (§35) is reported as
//! *degraded*, not *unready*.

mod probes;
mod readiness_state;
#[cfg(test)]
mod readiness_state_tests;
mod routes;
mod state;

pub use routes::health_routes;
pub use state::HealthState;
