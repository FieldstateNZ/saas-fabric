//! What one of a component's images is for.
use crate::errors::{invalid, ContractError};

contract_newtype!(
    /// The role an image plays in a component, such as `runtime` or
    /// `controlPlane`: an identifier, the same rule `components.yaml` pins
    /// its images by, so the two can be compared name for name.
    Role,
    check
);

/// The rule: `fabric_core::naming::parse_identifier`.
fn check(value: &str) -> Result<(), ContractError> {
    fabric_core::naming::parse_identifier("role", value)
        .map(drop)
        .map_err(|error| invalid(error.to_string()))
}
