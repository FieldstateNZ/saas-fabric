//! The rules the catalogue and a component descriptor share, moved here from
//! `fabric-client-model` (ADR 0026 section 2).
//!
//! # Why every message is frozen
//!
//! These are the catalogue's own validators: its console shows their
//! messages verbatim, and tests in `fabric-client-model` and
//! `fabric-control-plane` assert on them. Moving them changed where they
//! live, not one byte of what they say, and a change to a message here is a
//! change to the catalogue's API.
//!
//! # Why the rules only relax
//!
//! A published component descriptor cannot be edited, and a catalogue stores
//! frozen copies of what it declares. Within v1 a rule here may only accept
//! more; a stricter rule is a new component descriptor version.
mod fields;
mod names;
mod resource;
mod values;
use crate::errors::{invalid, ContractError};
pub use fields::{check_key, validate_fields};
pub use names::{is_hostname, is_identifier};
pub use resource::validate_resources;
use std::collections::BTreeSet;
pub use values::{check_value, is_timezone};

/// Checks `value` is text of at most `max` bytes with no control
/// characters, and not blank when `required`. `label` names it in the
/// message.
///
/// # Errors
///
/// Returns [`ContractError::Invalid`] saying what the text must be.
pub fn text(value: &str, label: &str, required: bool, max: usize) -> Result<(), ContractError> {
    if (required && value.trim().is_empty()) || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{label} must be {}text of at most {max} bytes without control characters",
            if required { "nonempty " } else { "" }
        )));
    }
    Ok(())
}

/// Checks no value repeats. `label` names what repeated in the message.
///
/// # Errors
///
/// Returns [`ContractError::Invalid`] naming the first repeated value.
pub fn unique<'a>(values: impl Iterator<Item = &'a str>, label: &str) -> Result<(), ContractError> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(invalid(format!("Duplicate {label}: {value}")));
        }
    }
    Ok(())
}
