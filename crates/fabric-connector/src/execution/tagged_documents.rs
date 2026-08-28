//! The document shapes [`ConnectionSelector`] and [`IsolationModel`] are read
//! through, so that surplus fields are refused on *every* variant.
//!
//! # Why these mirrors exist at all
//!
//! Both types are internally tagged enums, and the obvious way to refuse a
//! surplus field on one is `#[serde(deny_unknown_fields)]`. That attribute
//! does what it says for variants that have fields, and **silently does
//! nothing for variants that do not.** Serde deserialises an internally tagged
//! unit variant by reading the tag and discarding the rest of the map; there is
//! no field list to compare the surplus against, so no error is raised. Both
//! enums have exactly one such variant, and in each case it is the one whose
//! silent acceptance does the damage.
//!
//! The consequence is not academic. These two documents both parsed clean
//! before this module existed:
//!
//! ```text
//! {"kind": "default",  "name": "acme-prod"}                        -> Default
//! {"kind": "database", "column": "tenant_key", "value": "t-482"}   -> Database
//! ```
//!
//! The first discards an operator's choice of connection and substitutes "the
//! connector's one database", which is a claim about infrastructure they never
//! made. The second is an operator who believes they configured discriminator
//! isolation: what they actually get contributes no predicate at all.
//!
//! # Why a mirror rather than changing the public enums
//!
//! Declaring the public variants as empty struct variants — `Default {}` — is
//! the direct fix, and it does work: an empty field list is still a field list,
//! so `deny_unknown_fields` engages. But it changes every pattern match in the
//! workspace from `ConnectionSelector::Default` to `ConnectionSelector::Default
//! {}`, in crates that have nothing to do with parsing, to buy a property that
//! only matters during deserialisation.
//!
//! So the empty-struct form lives here, on types that exist only to be
//! deserialised, and the public enums keep the unit variants that read
//! naturally at a match site. `#[serde(from = "...")]` on each public enum is
//! what joins the two.
//!
//! Serialization is deliberately *not* routed through these types. The public
//! enums still derive `Serialize` directly, and a unit variant and an empty
//! struct variant emit the identical `{"kind": "..."}` either way — pinned by
//! round-trip tests beside each enum.
//!
//! # Keeping the mirrors honest
//!
//! Nothing here has to be remembered. Adding a variant to either public enum
//! makes the `From` impl below a non-exhaustive match, which is a compile
//! error, so the mirror cannot fall behind the type it mirrors.

use crate::{ConnectionName, FieldName, IsolationModel, SchemaName, SecretRef};

use super::ConnectionSelector;

/// The wire shape of a [`ConnectionSelector`].
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum ConnectionSelectorDocument {
    /// See [`ConnectionSelector::Default`]. Empty braces are load-bearing.
    Default {},
    /// See [`ConnectionSelector::Named`].
    Named {
        /// The connection's name in the connector's configuration.
        name: ConnectionName,
    },
    /// See [`ConnectionSelector::Secret`].
    Secret {
        /// Where to find the credential (§21).
        reference: SecretRef,
    },
}

impl From<ConnectionSelectorDocument> for ConnectionSelector {
    fn from(document: ConnectionSelectorDocument) -> Self {
        match document {
            ConnectionSelectorDocument::Default {} => Self::Default,
            ConnectionSelectorDocument::Named { name } => Self::Named { name },
            ConnectionSelectorDocument::Secret { reference } => Self::Secret { reference },
        }
    }
}

/// The wire shape of an [`IsolationModel`].
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum IsolationModelDocument {
    /// See [`IsolationModel::Database`]. Empty braces are load-bearing.
    Database {},
    /// See [`IsolationModel::Schema`].
    Schema {
        /// The tenant's schema.
        schema: SchemaName,
    },
    /// See [`IsolationModel::Discriminator`].
    Discriminator {
        /// The column holding the tenant discriminator.
        column: FieldName,
        /// This tenant's value in that column.
        value: String,
    },
}

impl From<IsolationModelDocument> for IsolationModel {
    fn from(document: IsolationModelDocument) -> Self {
        match document {
            IsolationModelDocument::Database {} => Self::Database,
            IsolationModelDocument::Schema { schema } => Self::Schema { schema },
            IsolationModelDocument::Discriminator { column, value } => Self::Discriminator { column, value },
        }
    }
}
