//! Identifiers this crate validates itself, because they live in the runtime
//! plane and this crate may not depend on it.
//!
//! [`fabric_core::naming`] is the shared rule set both sides use, so a value
//! this crate's [`ConnectorId`] accepts is a value
//! `fabric_connector::ConnectorId` accepts too — the two copies ask the
//! identical question and cannot silently diverge into two character sets.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invalid_identifier_is_refused_at_construction_not_at_the_runtime() {
        // Each of these five types also lives in the runtime plane, under
        // `fabric-connector`. There, a bad string is only caught when the
        // published file is deserialised at startup or refresh. Re-declaring
        // the type here, over the same parse function, moves the failure to
        // the moment this crate builds the value — long before any byte is
        // written to disk.
        assert!(ConnectorId::try_new("Not An Identifier!").is_err());
        assert!(ConnectionName::try_new("").is_err());
        assert!(FieldName::try_new("1-starts-with-a-digit").is_err());
    }

    #[test]
    fn an_invalid_collection_name_is_refused_at_construction() {
        assert!(CollectionName::try_new("").is_err());
        assert!(CollectionName::try_new("customers table").is_err());
    }

    #[test]
    fn an_invalid_schema_name_is_refused_at_construction() {
        assert!(SchemaName::try_new("").is_err());
        assert!(SchemaName::try_new("1-starts-with-a-digit").is_err());
    }
}
