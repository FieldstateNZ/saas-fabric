//! Turning declared data sources, recorded placements and the derived
//! runtime catalogue into the three documents `fabric-runtime-publication`
//! writes, and doing that on a schedule the control plane drives.
//!
//! ADR 0023 part 4: publication is a controller, not a one-shot script. It
//! reads what the platform has declared and recorded, composes a complete
//! `RuntimeSnapshot`, and offers it to whatever implements
//! `RuntimePublication` -- a filesystem today, Kubernetes `ConfigMap`s in
//! production (ADR 0018, "The Kubernetes adapter"). Composition
//! ([`compose`]) is pure and knows nothing about a schedule, an HTTP
//! handler, or a service account; [`RuntimePublisher`] is the one thing
//! here that touches a port, and only the ports this crate already owns
//! plus the one new seam, [`RuntimeCatalogueSource`], that lets it reach a
//! derived catalogue without this crate depending on `fabric-client-model`.
//!
//! # What is not here
//!
//! The control plane's implementation of [`RuntimeCatalogueSource`], the
//! `/api/platform` row, the operator trigger, the schedule that calls
//! [`RuntimePublisher::publish_once`] on an interval, and the Kubernetes
//! adapter itself are ADR 0023 part 4's other half -- named in the ADR,
//! built against the public surface this module exports, not built here.

mod catalogue_source;
mod compose_error;
mod outcome;
mod pass;
mod protocol;
mod publish_error;
mod publisher;
mod reads;
mod running_guard;
mod snapshot;
mod state;

#[cfg(test)]
pub(crate) mod testing;

pub use catalogue_source::{CatalogueSourceError, RuntimeCatalogueSource};
pub use compose_error::ComposeError;
pub use outcome::{PassOutcome, PassResult, WaitingReason};
pub use publisher::RuntimePublisher;
pub use snapshot::compose;
pub use state::{LastPass, PublicationState};
