//! What the connector told us it has, indexed for the request path.

mod collection_index;
mod operator_fit;
mod operator_index;
mod procedure_index;
#[cfg(test)]
mod schema_index_tests;
mod schema_index_type;
mod semantic_operator;

use operator_fit::OperatorFit;

pub(crate) use procedure_index::ArgumentKind;
pub use schema_index_type::SchemaIndex;
pub use semantic_operator::SemanticOperator;
