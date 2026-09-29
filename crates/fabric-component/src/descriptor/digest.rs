//! What one image is, exactly.
use crate::errors::{invalid, ContractError};

contract_newtype!(
    /// A content digest: `sha256:` and 64 lower-case hexadecimal characters.
    ///
    /// Only `sha256` is accepted (ADR 0026 section 3), and only in lower
    /// case, so one digest has one spelling and two can be compared as
    /// strings.
    Digest,
    check
);

/// The algorithm prefix every digest carries.
const PREFIX: &str = "sha256:";

/// The number of hexadecimal characters a `sha256` digest has.
const HEX_LENGTH: usize = 64;

/// The rule.
fn check(value: &str) -> Result<(), ContractError> {
    let hex = value.strip_prefix(PREFIX).unwrap_or_default();
    let valid = hex.len() == HEX_LENGTH
        && hex
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(invalid(
            "A digest must be sha256: followed by 64 lower-case hexadecimal characters",
        ))
    }
}
