//! One record on its way out — and the only place a row becomes a response.

use fabric_connector::Row;
use serde_json::{Map, Value};

use crate::models::VisibleFields;

/// One record, as JSON.
///
/// A plain object rather than a typed struct: the Data API is generic over
/// resources, and the shape of a row is the collection's business.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(transparent)]
pub struct RowResponse(Map<String, Value>);

impl RowResponse {
    /// Builds a response row carrying only the fields this request may disclose.
    ///
    /// # Why this is the only constructor
    ///
    /// There was a `From<&Row>` here, and it was the hole. `queryable_fields`
    /// was checked on everything a caller could *ask for* — `select`, filters,
    /// sort, and written field names — and on nothing that came *back*. A list
    /// or a read-by-key with no `select` left
    /// [`QuerySpec::fields`](fabric_connector::QuerySpec) empty, which that
    /// type documents as "whatever the connector considers the default
    /// projection — usually all fields", and every one of them was serialised
    /// straight to the caller. So `?select=salary` answered 400 and the bare
    /// request answered 200 with the salary in it.
    ///
    /// On a shared table the same body also carried the discriminator column
    /// and the tenant's internal key — handing an application the isolation
    /// model §26 says it must be unaware of, by name and by value.
    ///
    /// Deleting `From<&Row>` is what makes the fix hold rather than merely
    /// applying it today. A projection each call site has to remember is a rule
    /// that drifts the first time someone adds a handler; a constructor that
    /// will not compile without a [`VisibleFields`] in hand is one the compiler
    /// keeps. And a `VisibleFields` can only be built from a resolved
    /// operation, so the rules applied here are always the ones this request
    /// was authorised and placed under.
    ///
    /// Note that the key field is not special-cased. A resource whose
    /// `queryable_fields` omits its own `key_field` can still be read by key —
    /// the predicate is built by the platform, not the caller — but the key
    /// will not appear in the body. That is the catalogue's stated intent, not
    /// an accident of this function.
    pub(crate) fn project(row: &Row, visible: &VisibleFields<'_>) -> Self {
        Self(
            row.as_map()
                .iter()
                .filter(|(field, _)| visible.permits(field))
                .map(|(field, value)| (field.to_string(), value.clone()))
                .collect(),
        )
    }

    /// Borrows the underlying object.
    #[must_use]
    pub const fn as_map(&self) -> &Map<String, Value> {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use fabric_connector::{FieldName, IsolationModel};

    use super::*;
    use crate::ResourceDefinition;

    fn resource(json: &str) -> ResourceDefinition {
        serde_json::from_str(json).unwrap()
    }

    fn wide_row() -> Row {
        Row::new()
            .with(FieldName::try_new("id").unwrap(), Value::from(1))
            .with(FieldName::try_new("salary").unwrap(), Value::from(190_000))
            .with(
                FieldName::try_new("tenant_key").unwrap(),
                Value::String("tenant-482".to_owned()),
            )
    }

    #[test]
    fn an_unrestricted_resource_on_a_dedicated_database_passes_every_field_through() {
        let open = resource(r#"{"data_source":"primary","collection":"customers"}"#);
        let visible = VisibleFields::new(&open, &IsolationModel::Database);

        assert_eq!(RowResponse::project(&wide_row(), &visible).as_map().len(), 3);
    }

    #[test]
    fn a_restricted_resource_drops_a_field_it_does_not_expose() {
        let restricted = resource(
            r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#,
        );
        let visible = VisibleFields::new(&restricted, &IsolationModel::Database);

        let projected = RowResponse::project(&wide_row(), &visible);

        assert!(projected.as_map().contains_key("id"));
        assert!(!projected.as_map().contains_key("salary"));
    }

    #[test]
    fn a_shared_placement_drops_the_tenant_discriminator_column() {
        // The sharpest case: `tenant_key` names the isolation model and
        // carries the tenant's internal surrogate key (§26). The resource
        // here enumerates nothing, so only the isolation rule can catch it.
        let open = resource(r#"{"data_source":"primary","collection":"customers"}"#);
        let shared = IsolationModel::Discriminator {
            column: FieldName::try_new("tenant_key").unwrap(),
            value: "tenant-482".to_owned(),
        };
        let visible = VisibleFields::new(&open, &shared);

        let projected = RowResponse::project(&wide_row(), &visible);

        assert!(!projected.as_map().contains_key("tenant_key"));
        assert!(!serde_json::to_string(&projected).unwrap().contains("tenant-482"));
    }
}
