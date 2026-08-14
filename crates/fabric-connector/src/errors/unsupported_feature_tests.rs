//! Proving the published half of a refusal is closed, and the operator's half
//! is not published.
//!
//! These replace `fabric-data-api`'s former `neutral_feature` allowlist. That
//! table existed because `feature` was a `String` and the consuming side could
//! not trust what the producing side put in it. It is gone: the property is now
//! held by the type, and this is where the type is pinned.

use crate::{ComparisonOperator, ConnectorError, RefusalDetail, UnsupportedFeature};

/// Every variant, so the assertions below are exhaustive rather than a sample.
///
/// Kept honest by [`the_variant_list_is_complete`], which matches on each one
/// with no wildcard arm — adding a variant stops this file compiling, which is
/// how its author is pointed at the rule in the type's docs.
const ALL: [UnsupportedFeature; 18] = [
    UnsupportedFeature::Filtering,
    UnsupportedFeature::FilteringOnMutations,
    UnsupportedFeature::Ordering,
    UnsupportedFeature::Paging,
    UnsupportedFeature::Comparison(ComparisonOperator::Equal),
    UnsupportedFeature::Comparison(ComparisonOperator::NotEqual),
    UnsupportedFeature::Comparison(ComparisonOperator::LessThan),
    UnsupportedFeature::Comparison(ComparisonOperator::LessThanOrEqual),
    UnsupportedFeature::Comparison(ComparisonOperator::GreaterThan),
    UnsupportedFeature::Comparison(ComparisonOperator::GreaterThanOrEqual),
    UnsupportedFeature::Comparison(ComparisonOperator::Contains),
    UnsupportedFeature::Membership,
    UnsupportedFeature::NullComparison,
    UnsupportedFeature::Mutations,
    UnsupportedFeature::WritesToCollection,
    UnsupportedFeature::InsertsOnCollection,
    UnsupportedFeature::UpdatesOnCollection,
    UnsupportedFeature::DeletesOnCollection,
];

#[test]
fn the_variant_list_is_complete() {
    for feature in ALL {
        // No wildcard arm, deliberately.
        match feature {
            UnsupportedFeature::Filtering
            | UnsupportedFeature::FilteringOnMutations
            | UnsupportedFeature::Ordering
            | UnsupportedFeature::Paging
            | UnsupportedFeature::Membership
            | UnsupportedFeature::NullComparison
            | UnsupportedFeature::Mutations
            | UnsupportedFeature::WritesToCollection
            | UnsupportedFeature::InsertsOnCollection
            | UnsupportedFeature::UpdatesOnCollection
            | UnsupportedFeature::DeletesOnCollection => {}
            UnsupportedFeature::Comparison(operator) => match operator {
                ComparisonOperator::Equal
                | ComparisonOperator::NotEqual
                | ComparisonOperator::LessThan
                | ComparisonOperator::LessThanOrEqual
                | ComparisonOperator::GreaterThan
                | ComparisonOperator::GreaterThanOrEqual
                | ComparisonOperator::Contains => {}
            },
        }
    }
}

#[test]
fn every_name_is_a_distinct_non_empty_phrase() {
    let mut seen: Vec<&'static str> = ALL.iter().map(|feature| feature.as_str()).collect();
    seen.sort_unstable();

    let count = seen.len();
    seen.dedup();

    assert_eq!(seen.len(), count, "two variants render identically");
    assert!(seen.iter().all(|name| !name.is_empty()));
}

#[test]
fn no_name_reads_like_a_physical_identifier() {
    // The published vocabulary describes the capability the *caller* asked for,
    // never where it was needed. A name that reached for a location would have
    // to be built at runtime out of an identifier, which is what the leak was —
    // `comparing customer_records_v2.tenant_key with a Equal operator`.
    for feature in ALL {
        let name = feature.as_str();

        assert!(!name.contains('.'), "{name} reads as a qualified identifier");
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c == ' ' || c == '_'),
            "{name} is not plain lower-case prose"
        );
    }
}

#[test]
fn a_refusal_detail_never_reaches_the_rendered_error() {
    // The property `fabric-data-api`'s masking rests on: `Display` is the safe
    // rendering, so any message built from a `ConnectorError` — including one
    // built by a future arm nobody has written yet — carries no identifier.
    let error = UnsupportedFeature::Comparison(ComparisonOperator::Equal)
        .refused_because("customer_records_v2.tenant_key has no equal operator");

    let rendered = format!("{error}");

    assert_eq!(rendered, "connector does not support the equal comparison");
    assert!(!rendered.contains("customer_records_v2"));
    assert!(!rendered.contains("tenant_key"));
}

#[test]
fn the_detail_is_reachable_for_an_operator() {
    let error = UnsupportedFeature::WritesToCollection
        .refused_because("no procedure mapping for customer_records_v2");

    assert!(error.operator_message().contains("customer_records_v2"));
    assert!(error.operator_message().contains("writes to this collection"));
}

#[test]
fn a_refusal_with_no_detail_reads_the_same_either_way() {
    let error = UnsupportedFeature::Ordering.refused();

    assert_eq!(error.operator_message(), error.to_string());
    assert!(matches!(
        error,
        ConnectorError::Unsupported {
            feature: UnsupportedFeature::Ordering,
            ref detail,
        } if detail == &RefusalDetail::none()
    ));
}
