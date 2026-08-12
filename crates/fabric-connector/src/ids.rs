//! Validated names used at the connector boundary.
//!
//! Every one of these ends up in a position where an unvalidated string would
//! be dangerous — a SQL identifier, a schema qualifier, a collection name in a
//! generated query. They are all newtypes over the same rule set for that
//! reason.

#[macro_use]
mod identifier_newtype;

mod collection_name;
mod connection_name;
mod connector_id;
mod field_name;
mod schema_name;

pub use collection_name::CollectionName;
pub use connection_name::ConnectionName;
pub use connector_id::ConnectorId;
pub use field_name::FieldName;
pub use schema_name::SchemaName;
