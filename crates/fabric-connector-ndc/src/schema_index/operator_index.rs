//! Mapping platform semantics onto the connector's operator names.

use std::collections::{BTreeMap, BTreeSet};

use fabric_connector::ComparisonOperator;

use crate::schema_index::SemanticOperator;
use crate::wire::NdcSchemaResponse;

/// Scalar type name → semantic → the connector's name for it.
pub(super) type OperatorIndex = BTreeMap<String, BTreeMap<SemanticOperator, String>>;

/// Builds the operator index from a connector's schema.
///
/// Where a scalar declares two names with the same meaning, the first in name
/// order wins. Deterministic beats arbitrary: the generated query should not
/// change between restarts.
pub(super) fn build(schema: &NdcSchemaResponse) -> OperatorIndex {
    schema
        .scalar_types
        .iter()
        .map(|(scalar_name, scalar)| {
            let mut by_semantic = BTreeMap::new();

            for (operator_name, definition) in &scalar.comparison_operators {
                if let Some(semantic) = SemanticOperator::from_definition(definition) {
                    by_semantic
                        .entry(semantic)
                        .or_insert_with(|| operator_name.clone());
                }
            }

            (scalar_name.clone(), by_semantic)
        })
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
