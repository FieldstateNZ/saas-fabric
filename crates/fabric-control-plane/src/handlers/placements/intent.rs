//! Converting a client's declared data intent into the one the selector reads.
//!
//! `fabric_client_model::DataIntent` and `fabric_platform_management::DataIntent`
//! are structural twins that neither crate may depend on the other to
//! share -- see the latter's rustdoc for why. This handler is the one
//! caller with both types in scope (`docs/architecture/crate-dependencies.md`
//! gives that edge to `fabric-control-plane` alone), so it is where the two
//! meet. Both hold the same [`PlacementClassDocument`], so `class` copies
//! rather than translates.

use std::collections::BTreeMap;

use fabric_core::LogicalDataSourceName;

/// Converts one logical data source's intent.
///
/// An exhaustive destructure, not a field-by-field literal: if
/// `fabric_client_model::DataIntent` ever gains a field, this fails to
/// compile rather than silently dropping it on the floor.
pub(super) fn to_platform_intent(
    intent: &fabric_client_model::DataIntent,
) -> fabric_platform_management::DataIntent {
    let fabric_client_model::DataIntent {
        class,
        provider,
        region,
    } = intent.clone();

    fabric_platform_management::DataIntent {
        class,
        provider,
        region,
    }
}

/// Converts every logical data source a client's document names, for a
/// call that previews or places more than one at once.
pub(super) fn to_platform_intents(
    intents: &BTreeMap<LogicalDataSourceName, fabric_client_model::DataIntent>,
) -> BTreeMap<LogicalDataSourceName, fabric_platform_management::DataIntent> {
    intents
        .iter()
        .map(|(logical, intent)| (logical.clone(), to_platform_intent(intent)))
        .collect()
}

#[cfg(test)]
mod tests {
    use fabric_platform_management::PlacementClassDocument;

    use super::*;

    #[test]
    fn every_field_carries_across_unchanged() {
        let intent = fabric_client_model::DataIntent {
            class: PlacementClassDocument::Shared,
            provider: Some("postgres".to_owned()),
            region: Some("nz".to_owned()),
        };

        let converted = to_platform_intent(&intent);

        assert_eq!(converted.class, PlacementClassDocument::Shared);
        assert_eq!(converted.provider.as_deref(), Some("postgres"));
        assert_eq!(converted.region.as_deref(), Some("nz"));
    }

    #[test]
    fn every_logical_data_source_is_converted() {
        let mut intents = BTreeMap::new();
        intents.insert(
            LogicalDataSourceName::try_new("primary").unwrap(),
            fabric_client_model::DataIntent {
                class: PlacementClassDocument::Dedicated,
                provider: None,
                region: None,
            },
        );

        let converted = to_platform_intents(&intents);

        assert_eq!(converted.len(), 1);
        assert_eq!(
            converted[&LogicalDataSourceName::try_new("primary").unwrap()].class,
            PlacementClassDocument::Dedicated
        );
    }
}
