//! Placing a client's data intent on a declared data source, and recording it.
//!
//! ADR 0023 part 2: a client's `spec.data.<logical>` is intent, not
//! placement. Placing it is a Fabric write -- the selector in this module
//! chooses a declared data source that admits the intent, and the outcome
//! is recorded in `environments/<environment>/placements.yaml`, beside the
//! data sources it refers to (part 1, `crate::data_sources`). Nothing here
//! is published to the runtime and nothing here unplaces a tenant; both are
//! later work ADR 0023 names and does not decide.

pub(crate) mod held;
mod intent;
mod outcome;
mod port;
mod read;
mod record;
mod refusal;
mod select;
mod service;
mod tenant_id;

pub use intent::DataIntent;
pub use outcome::{ClientPlacements, PlacementOutcome};
pub use port::PlacementState;
pub use read::PlacementsRead;
pub use record::PlacementRecord;
pub use refusal::PlacementRefusal;
pub use select::select;
pub use service::Placements;
