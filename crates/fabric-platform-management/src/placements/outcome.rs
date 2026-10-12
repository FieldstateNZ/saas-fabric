//! What `Placements::for_client` found, for every logical data source a
//! client's document names -- without writing anything.

use std::collections::{BTreeMap, BTreeSet};

use fabric_core::LogicalDataSourceName;

use crate::placements::record::PlacementRecord;
use crate::placements::refusal::PlacementRefusal;
use crate::DesiredRevision;

/// Every logical data source a client's document asks about, and what is
/// true of each -- already placed, placeable, or refused and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientPlacements {
    /// The placements document's revision this was read at.
    pub revision: Option<DesiredRevision>,

    /// One outcome per logical data source the caller asked about, keyed
    /// the same way `spec.data` is.
    pub entries: BTreeMap<LogicalDataSourceName, PlacementOutcome>,

    /// Every logical data source this client's tenant has a recorded
    /// placement for, whether or not its document still names it: exactly
    /// what its composed tenant binding binds (G8). A catalogue resource on
    /// any other logical data source gets `UnboundDataSource` for this
    /// tenant at the runtime (ADR 0018 part 4).
    pub bound: BTreeSet<LogicalDataSourceName>,
}

/// What is true of one logical data source's intent, computed without
/// writing anything.
///
/// Three states, not two, because "nothing recorded yet, and `select` would
/// place it" and "nothing recorded, and `select` refuses it" are different
/// facts an operator acts on differently -- one shows a **Place** button,
/// the other shows the reason there is nothing to click. Collapsing them
/// into a single refusal would either invent a refusal for the placeable
/// case or hide the real one for the refused case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlacementOutcome {
    /// Already recorded.
    Placed(PlacementRecord),

    /// Nothing recorded yet, and a declared data source admits the intent.
    Placeable,

    /// Nothing recorded, and `select` would refuse this intent, for the
    /// reason given.
    Refused(PlacementRefusal),
}
