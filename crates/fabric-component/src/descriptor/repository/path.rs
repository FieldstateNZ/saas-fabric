//! A repository's path: the OCI distribution specification's grammar,
//! `[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*(/[a-z0-9]+((\.|_|__|-+)[a-z0-9]+)*)*`,
//! written out by hand rather than with a regular-expression crate.
use crate::errors::{invalid, ContractError};

/// Checks every `/`-separated component of `path`.
pub(super) fn check(path: &str) -> Result<(), ContractError> {
    if path.split('/').all(is_component) {
        Ok(())
    } else {
        Err(invalid(format!(
            "A repository path is lower-case letters and digits, separated by '.', '_', '__' or hyphens, in '/'-separated components: {path}"
        )))
    }
}

/// One path component: runs of `[a-z0-9]` joined by exactly one separator
/// -- `.`, `_`, `__`, or one or more `-` -- starting and ending with a run.
fn is_component(component: &str) -> bool {
    let is_run_byte = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    let bytes = component.as_bytes();
    let starts_and_ends_with_a_run =
        bytes.first().copied().is_some_and(is_run_byte) && bytes.last().copied().is_some_and(is_run_byte);
    starts_and_ends_with_a_run
        && component
            .split(|character: char| character.is_ascii_lowercase() || character.is_ascii_digit())
            .filter(|separator| !separator.is_empty())
            .all(is_separator)
}

/// A separator between two runs.
fn is_separator(separator: &str) -> bool {
    matches!(separator, "." | "_" | "__") || separator.bytes().all(|byte| byte == b'-')
}
