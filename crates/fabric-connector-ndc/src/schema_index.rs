//! What the connector told us it has, indexed for the request path.

mod collection_index;
mod operator_index;
#[cfg(test)]
mod schema_index_tests;
mod schema_index_type;
mod semantic_operator;

pub use schema_index_type::SchemaIndex;
pub use semantic_operator::SemanticOperator;
