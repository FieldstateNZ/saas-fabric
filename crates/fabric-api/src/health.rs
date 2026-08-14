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
//! should fail when the reconciled state it holds can serve no tenant — which
//! means both that a registry has not primed *and* that it primed empty while
//! the other did not (§28). It should **not** fail merely because one of
//! several connectors is unhealthy, nor because one is slow to answer — see
//! the `readiness_state` submodule for that decision and why partial connector
//! failure (§35) is reported as *degraded*, not *unready*.
//!
//! # Three properties, three submodules
//!
//! The probe is short but it has to get three separate things right, and each
//! was wrong in a different way for a different reason:
//!
//! - **What the verdict is** — `readiness_state`, over the facts in
//!   `readiness_facts`. Priming is necessary and not sufficient.
//! - **How long it takes to reach one** — `connector_sweep`. Concurrent and
//!   deadline-bounded, because a probe that cannot answer inside a kubelet's
//!   one-second budget is a failed probe whatever it would eventually have
//!   said.
//! - **Who is told the detail** — `detail_access` and `readiness_body`. The
//!   verdict is public; connector ids, estate size, and backend messages are
//!   not.

mod connector_health;
mod connector_sweep;
mod detail_access;
mod logging;
mod probes;
mod readiness_body;
mod readiness_facts;
mod readiness_state;
#[cfg(test)]
mod readiness_state_tests;
mod routes;
mod state;

#[cfg(test)]
mod connector_sweep_tests;
#[cfg(test)]
mod readiness_body_tests;

pub use routes::health_routes;
pub use state::HealthState;
