//! One logical data source's declared intent, as `spec.data.<logical>` states it.

use fabric_runtime_publication::PlacementClassDocument;

/// What `spec.data.<logical>` asks for: intent, not placement.
///
/// # This is not a `DataSourceDeclaration`
///
/// A `DataIntent` says what a client's document wants; whether anything
/// declared in the environment can satisfy it is a question the selector
/// answers later, against `fabric-platform-management`'s declared data
/// sources (ADR 0023 part 2). Nothing here reaches Git, and nothing here
/// knows what an environment is.
///
/// `class` reuses the wire's own [`PlacementClassDocument`] rather than a
/// third declaration of the same six values, so `spec.data`'s `class` can
/// never name a placement class a data source could not also declare — see
/// this crate's `Cargo.toml` for why that edge is allowed at all. Its
/// `snake_case` spelling means `high_availability`, not the hyphenated
/// `high-availability` an earlier draft of the document format used; see
/// `docs/architecture/client-desired-state.md`'s `spec.data` section.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataIntent {
    /// The service class this intent asks for.
    pub class: PlacementClassDocument,

    /// Free text, carried and shown, never matched.
    ///
    /// Nothing in the platform declares a provider on a data source yet, so
    /// there is nothing here could match against; a refusal that could not
    /// place this intent says so when a provider was stated, rather than
    /// implying the provider was the reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    /// Matched against a candidate data source's `residency.region` when
    /// present, exactly. Absent means any region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_fields_are_refused() {
        let error = serde_norway::from_str::<DataIntent>(
            r"
class: dedicated
bogus: true
",
        )
        .unwrap_err();

        assert!(error.to_string().contains("bogus"));
    }

    #[test]
    fn the_wires_snake_case_spelling_is_accepted() {
        let intent: DataIntent = serde_norway::from_str(
            r"
class: high_availability
",
        )
        .unwrap();

        assert_eq!(intent.class, PlacementClassDocument::HighAvailability);
    }

    #[test]
    fn the_documents_earlier_hyphenated_spelling_is_not_accepted() {
        let error = serde_norway::from_str::<DataIntent>(
            r"
class: high-availability
",
        )
        .unwrap_err();

        assert!(
            error.to_string().contains("high-availability") || error.to_string().contains("unknown variant")
        );
    }

    #[test]
    fn provider_and_region_default_to_absent() {
        let intent: DataIntent = serde_norway::from_str(
            r"
class: shared
",
        )
        .unwrap();

        assert_eq!(intent.provider, None);
        assert_eq!(intent.region, None);
    }
}
