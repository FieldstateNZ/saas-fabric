//! Which release of a component a descriptor describes.
use crate::errors::{invalid, ContractError};

contract_newtype!(
    /// A component's version: `SemVer` 2.0 without build metadata or a `v`
    /// prefix, at most 128 bytes -- `1.4.0`, `0.3.0-preview.14`.
    ///
    /// # Why so strict
    ///
    /// The version is equal byte for byte to the tag it was found by and to
    /// the image's `version` annotation (ADR 0026 section 2). A `+` cannot
    /// appear in an OCI tag at all, and a `v` prefix or a leading zero is a
    /// second spelling of a version that already has one; a reader comparing
    /// versions by their text must never meet two spellings of one release.
    ComponentVersion,
    check
);

/// The longest version, in bytes.
const MAX_LENGTH: usize = 128;

/// The rule.
fn check(value: &str) -> Result<(), ContractError> {
    if value.len() > MAX_LENGTH {
        return Err(invalid(format!(
            "A component version must be at most {MAX_LENGTH} bytes"
        )));
    }
    if value.starts_with(['v', 'V']) {
        return Err(invalid(
            "A component version is written without a v prefix, such as 1.4.0",
        ));
    }
    if value.contains('+') {
        return Err(invalid("A component version carries no build metadata"));
    }
    let (core, prerelease) = match value.split_once('-') {
        Some((core, prerelease)) => (core, Some(prerelease)),
        None => (value, None),
    };
    let core_parts: Vec<&str> = core.split('.').collect();
    let core_valid = core_parts.len() == 3 && core_parts.iter().all(|part| is_numeric(part));
    let prerelease_valid =
        prerelease.is_none_or(|prerelease| prerelease.split('.').all(is_prerelease_identifier));
    if core_valid && prerelease_valid {
        Ok(())
    } else {
        Err(invalid(
            "A component version must be SemVer: MAJOR.MINOR.PATCH with an optional -prerelease",
        ))
    }
}

/// A numeric identifier: digits, and no leading zero unless it is `0`.
fn is_numeric(part: &str) -> bool {
    !part.is_empty()
        && part.bytes().all(|byte| byte.is_ascii_digit())
        && (part == "0" || !part.starts_with('0'))
}

/// A prerelease identifier: non-empty `[0-9A-Za-z-]`, and a numeric one
/// without a leading zero.
fn is_prerelease_identifier(part: &str) -> bool {
    if part.is_empty()
        || !part
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return false;
    }
    !part.bytes().all(|byte| byte.is_ascii_digit()) || is_numeric(part)
}
