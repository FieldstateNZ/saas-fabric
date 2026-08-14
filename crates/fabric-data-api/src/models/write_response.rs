//! The result of a write.

use fabric_connector::MutationOutcome;

use crate::models::VisibleFields;
use crate::RowResponse;

/// The result of a write.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WriteResponse {
    /// How many records the operation affected.
    pub affected: u64,

    /// Records the backend returned, if any — typically the written rows with
    /// server-generated keys filled in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<RowResponse>,
}

impl WriteResponse {
    /// Builds a write response from a mutation outcome.
    ///
    /// # Why a write needs the projection too
    ///
    /// `returned_rows` is the write path's version of the read path's default
    /// projection, and it was leaking for the same reason: nothing gated it.
    /// A connector implementing `RETURNING` hands back the stored row, which on
    /// a shared table includes the columns the resource hides *and* the tenant
    /// discriminator — so a `POST` disclosed exactly what a `GET` did. The
    /// allowlist a caller cannot write through
    /// (`execution::row_mapping::to_row`) has to be the same allowlist they
    /// cannot read back through.
    pub(crate) fn from_outcome(outcome: &MutationOutcome, visible: &VisibleFields<'_>) -> Self {
        Self {
            affected: outcome.affected_rows,
            data: outcome
                .returned_rows
                .iter()
                .map(|row| RowResponse::project(row, visible))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use fabric_connector::{FieldName, IsolationModel, Row};
    use serde_json::Value;

    use super::*;
    use crate::ResourceDefinition;

    fn resource(json: &str) -> ResourceDefinition {
        serde_json::from_str(json).unwrap()
    }

    fn open() -> ResourceDefinition {
        resource(r#"{"data_source":"primary","collection":"customers"}"#)
    }

    fn restricted() -> ResourceDefinition {
        resource(r#"{"data_source":"primary","collection":"customers","queryable_fields":["id","name"]}"#)
    }

    /// A dedicated placement, so these tests exercise the catalogue rule alone.
    /// The isolation rule has its own tests in `visible_fields`.
    fn dedicated(resource: &ResourceDefinition) -> VisibleFields<'_> {
        VisibleFields::new(resource, &IsolationModel::Database)
    }

    fn returned_row() -> Row {
        Row::new()
            .with(FieldName::try_new("id").unwrap(), Value::from(1))
            .with(FieldName::try_new("salary").unwrap(), Value::from(190_000))
            .with(
                FieldName::try_new("tenant_key").unwrap(),
                Value::String("tenant-482".to_owned()),
            )
    }

    #[test]
    fn a_write_response_omits_data_when_nothing_was_returned() {
        let resource = open();
        let outcome = MutationOutcome::affected(3);

        let json =
            serde_json::to_value(WriteResponse::from_outcome(&outcome, &dedicated(&resource))).unwrap();

        assert_eq!(json["affected"], 3);
        assert!(json.get("data").is_none());
    }

    #[test]
    fn a_returning_connector_cannot_hand_back_a_hidden_column() {
        let resource = restricted();
        let outcome = MutationOutcome::affected(1).with_rows(vec![returned_row()]);

        let json =
            serde_json::to_value(WriteResponse::from_outcome(&outcome, &dedicated(&resource))).unwrap();

        assert_eq!(json["data"][0]["id"], Value::from(1));
        assert_eq!(json["data"][0].get("salary"), None);
    }

    #[test]
    fn an_unrestricted_resource_still_returns_what_the_backend_sent() {
        let resource = open();
        let outcome = MutationOutcome::affected(1).with_rows(vec![returned_row()]);

        let response = WriteResponse::from_outcome(&outcome, &dedicated(&resource));

        assert_eq!(response.data.len(), 1);
        assert_eq!(response.data[0].as_map().len(), 3);
    }
}
