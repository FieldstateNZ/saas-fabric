//! Identifiers this crate validates itself, because they live in the runtime
//! plane and this crate may not depend on it.
//!
//! [`fabric_core::naming`] is the shared rule set both sides use, so a value
//! this crate's [`ConnectorId`] accepts is a value
//! `fabric_connector::ConnectorId` accepts too — the two copies ask the
//! identical question and cannot silently diverge into two character sets.
//!
//! `fabric_connector::ids::identifier_newtype` already generates this exact
//! shape, but it is a macro private to that crate and this crate may not
//! depend on `fabric-connector` to reach it (the same plane boundary that
//! makes every type below a separate declaration in the first place) — so
//! the macro is declared again here, over the same five names, rather than
//! hand-writing the boilerplate five times.

/// Declares a validated identifier newtype.
///
/// See `fabric_connector::ids::identifier_newtype` for the macro this one
/// mirrors: a checked constructor, `Display`, serde that validates on the
/// way in, and the ordering and hashing derives that let a value be a map
/// key. Each use site below is still its own file with its own rustdoc; only
/// the boilerplate is shared, so the types stay individually discoverable.
macro_rules! identifier_newtype {
    ($(#[$meta:meta])* $name:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Parses the name, enforcing the shared identifier character set.
            ///
            /// # Errors
            ///
            /// Returns [`fabric_core::IdentifierError`] if the value is empty,
            /// longer than 63 bytes, does not start with an ASCII letter, or
            /// contains a character outside letters, digits, hyphens, and
            /// underscores.
            pub fn try_new(value: impl AsRef<str>) -> Result<Self, fabric_core::IdentifierError> {
                fabric_core::naming::parse_identifier($kind, value.as_ref()).map(Self)
            }

            /// Borrows the name as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = fabric_core::IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::try_new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

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
