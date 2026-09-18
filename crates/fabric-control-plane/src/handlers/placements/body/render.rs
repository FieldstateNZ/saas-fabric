//! Turning a client's declared intents and computed outcomes into what the
//! console reads.

use std::collections::BTreeMap;

use fabric_core::LogicalDataSourceName;
use fabric_platform_management::{DesiredRevision, PlacementOutcome, PlacementRecord};

use super::{IntentRow, PlacedRow, PlacementRow, PlacementsBody};
use crate::handlers::platform::data_sources::placement;

impl PlacementsBody {
    /// Renders a client's placements at the given revision.
    ///
    /// `intents` is the client's own `spec.data`, in display order;
    /// `outcomes` is what [`Placements::for_client`](fabric_platform_management::Placements::for_client)
    /// computed for the same keys. Every `intents` entry gets a row, even
    /// one `outcomes` has nothing for -- which cannot happen in practice,
    /// since both are built from the same map, but a row with `placed` and
    /// `refusal` both absent is what that would mean rather than a panic.
    pub(in crate::handlers::placements) fn of(
        client_id: &str,
        environment: &str,
        revision: &DesiredRevision,
        intents: &BTreeMap<LogicalDataSourceName, fabric_client_model::DataIntent>,
        outcomes: &BTreeMap<LogicalDataSourceName, PlacementOutcome>,
    ) -> Self {
        Self {
            client_id: client_id.to_owned(),
            environment: environment.to_owned(),
            revision: revision.as_str().to_owned(),
            placements: intents
                .iter()
                .map(|(logical, intent)| PlacementRow::of(logical, intent, outcomes.get(logical)))
                .collect(),
        }
    }
}

impl PlacementRow {
    /// Renders one logical data source's intent and outcome.
    fn of(
        logical: &LogicalDataSourceName,
        intent: &fabric_client_model::DataIntent,
        outcome: Option<&PlacementOutcome>,
    ) -> Self {
        let (placed, refusal) = match outcome {
            Some(PlacementOutcome::Placed(record)) => (Some(PlacedRow::of(record)), None),
            Some(PlacementOutcome::Placeable) | None => (None, None),
            Some(PlacementOutcome::Refused(refusal)) => (None, Some(refusal.to_string())),
        };

        Self {
            logical: logical.to_string(),
            intent: IntentRow {
                class: placement::console_word(intent.class),
                provider: intent.provider.clone(),
                region: intent.region.clone(),
            },
            placed,
            refusal,
        }
    }
}

impl PlacedRow {
    /// Renders one recorded placement.
    fn of(record: &PlacementRecord) -> Self {
        Self {
            data_source: record.data_source.to_string(),
            isolation: record.isolation.clone(),
            placed_at: record.placed_at.clone(),
        }
    }
}
