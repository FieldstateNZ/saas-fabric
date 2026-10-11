//! Operation to relation: the one translation ADR 0013 gives the adapter.

use fabric_data_api::OperationKind;

/// The computed relation that answers whether `operation` is permitted.
///
/// ADR 0013 maps a client's declared relations (`viewer`, `editor`, …) onto
/// direct relations plus a computed `can_<operation>` per operation, so the
/// question the Data API asks is never "is the caller an editor?" — which
/// would hard-code what an editor may do — but "can the caller update?".
/// Which declared relations grant that is the model's business.
#[must_use]
pub fn computed_relation(operation: OperationKind) -> String {
    format!("can_{}", operation.as_str())
}

#[cfg(test)]
mod tests {
    use fabric_core::RelationName;

    use super::*;

    #[test]
    fn each_operation_asks_its_own_computed_relation() {
        let relations: Vec<String> = OperationKind::ALL.into_iter().map(computed_relation).collect();

        assert_eq!(
            relations,
            ["can_read", "can_list", "can_create", "can_update", "can_delete"]
        );
    }

    #[test]
    fn every_computed_relation_is_a_valid_relation_name() {
        for operation in OperationKind::ALL {
            assert!(
                RelationName::try_new(computed_relation(operation)).is_ok(),
                "{operation}"
            );
        }
    }
}
