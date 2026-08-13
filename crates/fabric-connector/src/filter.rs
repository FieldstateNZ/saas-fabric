//! The neutral predicate AST.
//!
//! Deliberately small. It expresses what the Data API lets applications ask
//! for, not everything a database can do — a richer AST would be harder to
//! translate faithfully to every backend, and every unfaithful translation is a
//! potential data leak.

mod comparison_operator;
mod filter_expression;
#[cfg(test)]
mod filter_expression_tests;
mod filter_introspection;
#[cfg(test)]
mod filter_introspection_tests;

pub use comparison_operator::ComparisonOperator;
pub use filter_expression::Filter;
