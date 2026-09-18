//! Why the selector would not place an intent, in the platform's own words.
//!
//! In the 121-150 line band (docs/architecture/file-size-policy.md): one
//! enum, `PlacementRefusal`, together with the message-building helpers
//! its non-trivial `Display` impls call -- `class_word` and
//! `no_data_source_admits_message` exist only because two of this enum's
//! variants need more than a format string, and neither would be reused or
//! tested apart from the refusal it renders.

use std::fmt::Write as _;

use fabric_core::{DataSourceId, LogicalDataSourceName, TenantId};
use fabric_runtime_publication::PlacementClassDocument;

/// Why [`select`](crate::select) would not place an intent.
///
/// Each variant's Display is the message an operator reads, and none of
/// them names a file: a placement is refused before anything is written, the
/// same reason [`DataSourceRule`](crate::DataSourceRule) names no path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlacementRefusal {
    /// This tenant's logical data source is already recorded placed.
    ///
    /// The selector's first rule, checked before any candidate is even
    /// considered -- a tenant is placed once, and a second attempt is not a
    /// different placement to compute, it is the same fact asked for again.
    #[error("{tenant} is already placed for {logical}")]
    AlreadyPlaced {
        /// The tenant already placed.
        tenant: TenantId,
        /// The logical data source it is already placed for.
        logical: LogicalDataSourceName,
    },

    /// No declared data source admits this intent at all: none is the
    /// right class, none accepts new tenants, none is writable, or none is
    /// in the stated region.
    #[error("{}", no_data_source_admits_message(*class, region.as_deref(), provider.as_deref()))]
    NoDataSourceAdmits {
        /// The class the intent asked for.
        class: PlacementClassDocument,
        /// The region the intent asked for, if it stated one.
        region: Option<String>,
        /// The provider the intent stated, if any -- carried into the
        /// message so an operator does not read the refusal as being about
        /// a provider nothing was ever going to match.
        provider: Option<String>,
    },

    /// At least one declared data source matches this intent's class,
    /// region and capabilities, but -- because the class is not `shared` --
    /// every one that matches already has a tenant.
    ///
    /// Kept apart from [`NoDataSourceAdmits`](Self::NoDataSourceAdmits): that variant tells an
    /// operator what to declare, and there is nothing to declare here --
    /// what is missing is provisioning, which ADR 0023 explicitly does not
    /// decide (part 2, "What this does not decide"). Telling an operator to
    /// declare a dedicated data source they already declared sends them
    /// looking for a mistake that is not theirs.
    #[error("every {} data source that matches is already somebody's tenant", class_word(*class))]
    AllMatchingSourcesOccupied {
        /// The class the intent asked for.
        class: PlacementClassDocument,
    },

    /// The client id this placement was asked for is not a valid tenant id.
    ///
    /// Cannot happen for a client the console created -- `ClientId` and
    /// `TenantId` validate with the same rule -- but the placements record
    /// is what the runtime reads, so this is checked at the one place that
    /// matters rather than trusted from the caller.
    #[error("{client} is not a valid tenant id")]
    TenantIdInvalid {
        /// The client id, exactly as the caller supplied it.
        client: String,
    },

    /// A shared data source already has a tenant recorded with the
    /// discriminator value this placement would use.
    ///
    /// The value is the tenant id (ADR 0023 part 2), which cannot collide
    /// for two different tenants -- so this is checked anyway, and refused
    /// rather than silently trusted, the same discipline
    /// `data_sources::held::check_held` applies to a document nothing but a
    /// hand edit could have broken.
    #[error("{data_source} already has a tenant recorded with discriminator value '{value}'")]
    DiscriminatorValueTaken {
        /// The data source that already carries the value.
        data_source: DataSourceId,
        /// The value that collided.
        value: String,
    },
}

/// The words an operator reads for a placement class, in the platform's own
/// vocabulary rather than the wire's `snake_case` spelling.
///
/// A second copy of `data_sources::rule::placement_word`, deliberately: that
/// function is private to `data_sources`, and importing a private helper
/// across a sibling module is not what module privacy is for. Six short
/// arms are a smaller cost than a visibility change to a module this one
/// does not otherwise touch.
const fn class_word(class: PlacementClassDocument) -> &'static str {
    match class {
        PlacementClassDocument::Shared => "shared",
        PlacementClassDocument::Dedicated => "dedicated",
        PlacementClassDocument::HighAvailability => "high availability",
        PlacementClassDocument::Regulated => "regulated",
        PlacementClassDocument::Development => "development",
        PlacementClassDocument::Ephemeral => "ephemeral",
    }
}

/// Builds `NoDataSourceAdmits`'s message: what would have to be declared,
/// and -- when the intent stated one -- a note that a provider is carried
/// and never matched, so it is not read as the reason nothing was found.
fn no_data_source_admits_message(
    class: PlacementClassDocument,
    region: Option<&str>,
    provider: Option<&str>,
) -> String {
    let mut message = format!("declare a {} data source", class_word(class));

    if let Some(region) = region {
        let _ = write!(message, " in region {region}");
    }

    message.push_str(" that accepts new tenants");

    if let Some(provider) = provider {
        let _ = write!(
            message,
            " ('{provider}' was stated as a provider, but nothing declares one yet, so it was not matched)"
        );
    }

    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_names_the_class_and_region_when_stated() {
        let refusal = PlacementRefusal::NoDataSourceAdmits {
            class: PlacementClassDocument::Shared,
            region: Some("nz".to_owned()),
            provider: None,
        };

        assert_eq!(
            refusal.to_string(),
            "declare a shared data source in region nz that accepts new tenants"
        );
    }

    #[test]
    fn the_message_omits_region_when_the_intent_did_not_state_one() {
        let refusal = PlacementRefusal::NoDataSourceAdmits {
            class: PlacementClassDocument::Dedicated,
            region: None,
            provider: None,
        };

        assert_eq!(
            refusal.to_string(),
            "declare a dedicated data source that accepts new tenants"
        );
    }

    #[test]
    fn a_stated_provider_is_named_but_not_implied_to_be_the_reason() {
        let refusal = PlacementRefusal::NoDataSourceAdmits {
            class: PlacementClassDocument::Shared,
            region: None,
            provider: Some("postgres".to_owned()),
        };

        let message = refusal.to_string();
        assert!(message.contains("postgres"), "{message}");
        assert!(message.contains("not matched"), "{message}");
    }
}
