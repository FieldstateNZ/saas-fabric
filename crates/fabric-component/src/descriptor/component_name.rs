//! What a component is called.
use crate::errors::{invalid, ContractError};

contract_newtype!(
    /// A component's name, such as `reports`: a DNS label, because it names
    /// the component wherever the platform does -- in `components.yaml`, in
    /// a commit message, in a URL path.
    ComponentName,
    check
);

/// The rule: `fabric_core::naming::parse_dns_label`.
fn check(value: &str) -> Result<(), ContractError> {
    fabric_core::naming::parse_dns_label("component name", value)
        .map(drop)
        .map_err(|error| invalid(error.to_string()))
}
