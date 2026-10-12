//! The authorization model's shape, as the authorization service's
//! `WriteAuthorizationModel` body (schema 1.1) spells it.

use std::collections::BTreeMap;

use serde::Serialize;

/// The schema every model this crate writes is in.
const SCHEMA_VERSION: &str = "1.1";

/// A complete authorization model: every type, in a stable order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuthorizationModel {
    /// Always [`SCHEMA_VERSION`].
    schema_version: &'static str,

    /// The principal type first, then each declared resource's record type and
    /// collection type, in the order the client declares them.
    type_definitions: Vec<TypeDefinition>,
}

impl AuthorizationModel {
    /// A model holding `type_definitions`.
    pub(crate) const fn new(type_definitions: Vec<TypeDefinition>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            type_definitions,
        }
    }
}

/// One type and its relations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TypeDefinition {
    /// The type's name.
    #[serde(rename = "type")]
    pub(crate) name: String,

    /// Each relation and how it is computed.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) relations: BTreeMap<String, Userset>,

    /// Which subject types each direct relation may be written for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) metadata: Option<Metadata>,
}

/// How a relation is computed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) enum Userset {
    /// Held directly, by a written tuple.
    #[serde(rename = "this")]
    Direct(Empty),

    /// Held by whoever holds another relation on the same object.
    #[serde(rename = "computedUserset")]
    Computed {
        /// The other relation.
        relation: String,
    },

    /// Held by whoever holds any of these.
    #[serde(rename = "union")]
    Union {
        /// The alternatives.
        child: Vec<Userset>,
    },

    /// Held by whoever holds `base` and not `subtract`.
    #[serde(rename = "difference")]
    Difference {
        /// What is held.
        base: Box<Userset>,
        /// What is taken away.
        subtract: Box<Userset>,
    },
}

/// The service's `{}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Empty {}

/// A type's relation metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct Metadata {
    /// Per direct relation.
    pub(crate) relations: BTreeMap<String, RelationMetadata>,
}

/// Which subject types a direct relation may be written for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RelationMetadata {
    /// Only the principal type: no wildcard and no userset.
    pub(crate) directly_related_user_types: Vec<RelatedType>,
}

/// One subject type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RelatedType {
    /// Its name.
    #[serde(rename = "type")]
    pub(crate) name: &'static str,
}
