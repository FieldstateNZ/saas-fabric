//! Mapping platform semantics onto the connector's operator names.

use std::collections::{BTreeMap, BTreeSet};

use fabric_connector::ComparisonOperator;

use crate::schema_index::{OperatorFit, SemanticOperator};
use crate::wire::{NdcScalarType, NdcSchemaResponse};

/// Scalar type name → semantic → the connector's name for it.
pub(super) type OperatorIndex = BTreeMap<String, BTreeMap<SemanticOperator, String>>;

/// Builds the operator index from a connector's schema.
pub(super) fn build(schema: &NdcSchemaResponse) -> OperatorIndex {
    schema
        .scalar_types
        .iter()
        .map(|(scalar_name, scalar)| (scalar_name.clone(), best_names(scalar)))
        .collect()
}

/// Picks one operator name per semantic for a scalar type, preferring the
/// operator that means exactly what was asked for.
///
/// Two tie-breaks, in this order:
///
/// 1. **Fit.** An [`OperatorFit::Exact`] definition always displaces a
///    [`OperatorFit::Widened`] one. This is the rule that matters: `contains`
///    and `contains_insensitive` both answer to
///    [`SemanticOperator::Contains`], and without it whichever sorted first
///    won — so `_ilike` beat `_like` alphabetically and a caller asking for
///    containment got the case-insensitive predicate instead.
/// 2. **Name order.** Between two definitions of equal fit, the first in name
///    order wins. Deterministic beats arbitrary: the generated query should
///    not change between restarts.
fn best_names(scalar: &NdcScalarType) -> BTreeMap<SemanticOperator, String> {
    let mut best: BTreeMap<SemanticOperator, (OperatorFit, String)> = BTreeMap::new();

    for (operator_name, definition) in &scalar.comparison_operators {
        let Some((semantic, fit)) = SemanticOperator::from_definition(definition) else {
            continue;
        };

        let improves = best
            .get(&semantic)
            .is_none_or(|(kept, _)| *kept == OperatorFit::Widened && fit == OperatorFit::Exact);

        if improves {
            best.insert(semantic, (fit, operator_name.clone()));
        }
    }

    best.into_iter()
        .map(|(semantic, (_, name))| (semantic, name))
        .collect()
}

/// The union of neutral operators expressible on at least one scalar type.
///
/// The permissive answer, because
/// [`ConnectorCapabilities`](fabric_connector::ConnectorCapabilities) is a
/// single global set while support is genuinely per scalar type. The
/// authoritative per-field check happens at translation time and fails closed.
pub(super) fn supported_neutral_operators(index: &OperatorIndex) -> BTreeSet<ComparisonOperator> {
    SemanticOperator::neutral_candidates()
        .into_iter()
        .filter(|candidate| {
            let semantic = SemanticOperator::for_neutral(*candidate);
            index
                .values()
                .any(|by_semantic| by_semantic.contains_key(&semantic))
        })
        .collect()
}
