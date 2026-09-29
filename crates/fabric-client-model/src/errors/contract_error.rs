//! A refusal from the shared component contract, as a desired-state error.
use super::DesiredStateError;
use fabric_component::ContractError;

/// Every refusal the moved catalogue validators raise becomes the
/// `InvalidField { field: "catalogue", .. }` those validators raised before
/// they moved, with the same message -- so no caller, test or console can
/// tell they moved.
impl From<ContractError> for DesiredStateError {
    fn from(error: ContractError) -> Self {
        let detail = match error {
            ContractError::Invalid { detail } | ContractError::OtherRegistry { detail } => detail,
            unsupported @ ContractError::UnsupportedVersion { .. } => unsupported.to_string(),
        };
        Self::InvalidField {
            field: "catalogue",
            detail,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_invalid_value_keeps_its_message() {
        let error = DesiredStateError::from(ContractError::Invalid {
            detail: "Duplicate field: team".into(),
        });

        assert_eq!(
            error,
            DesiredStateError::InvalidField {
                field: "catalogue",
                detail: "Duplicate field: team".into()
            }
        );
        assert_eq!(error.to_string(), "catalogue: Duplicate field: team");
    }

    #[test]
    fn an_unsupported_version_is_named() {
        let error = DesiredStateError::from(ContractError::UnsupportedVersion { found: "v2".into() });

        assert!(error.to_string().contains("v2"), "{error}");
    }
}
