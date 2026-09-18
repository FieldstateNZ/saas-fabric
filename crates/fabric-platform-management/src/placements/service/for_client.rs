//! Previewing every logical data source a client's document names, without
//! writing anything.

use std::collections::BTreeMap;

use fabric_core::LogicalDataSourceName;

use crate::data_sources::held::check_held;
use crate::placements::held::check_held_placements;
use crate::placements::intent::DataIntent;
use crate::placements::outcome::{ClientPlacements, PlacementOutcome};
use crate::placements::select::select;
use crate::placements::service::Placements;
use crate::placements::tenant_id::tenant_id;
use crate::PlatformError;

impl Placements {
    /// Every logical data source `intents` names, and what is true of each
    /// -- already placed, placeable, or refused and why. Writes nothing.
    ///
    /// `client` is the client id, reparsed as a `TenantId` the same way
    /// [`place`](Self::place) does (ADR 0023 part 2, N11) -- see that
    /// method's rustdoc for why the control plane no longer does this
    /// conversion itself.
    ///
    /// # Errors
    ///
    /// `PlatformError` if the client id is not a valid tenant id, either
    /// held document cannot be read, or is no longer coherent
    /// (`InvalidHeldDataSources`, `InvalidHeldPlacements`).
    pub async fn for_client(
        &self,
        environment: &str,
        client: &str,
        intents: &BTreeMap<LogicalDataSourceName, DataIntent>,
    ) -> Result<ClientPlacements, PlatformError> {
        let tenant = tenant_id(client)?;

        let declared = self.repository().read_data_sources(environment).await?;
        check_held(&declared.declarations)?;

        let held = self.repository().read_placements(environment).await?;
        check_held_placements(&held.placements, &declared.declarations)?;

        let now = self.stamp()?;
        let mut entries = BTreeMap::new();

        for (logical, intent) in intents {
            let existing = held
                .placements
                .iter()
                .find(|placement| placement.tenant == tenant && &placement.logical == logical);

            let outcome = if let Some(existing) = existing {
                PlacementOutcome::Placed(existing.clone())
            } else {
                match select(
                    intent,
                    &tenant,
                    logical,
                    &declared.declarations,
                    &held.placements,
                    &now,
                ) {
                    Ok(_) => PlacementOutcome::Placeable,
                    Err(refusal) => PlacementOutcome::Refused(refusal),
                }
            };

            entries.insert(logical.clone(), outcome);
        }

        Ok(ClientPlacements {
            revision: held.revision,
            entries,
        })
    }
}
