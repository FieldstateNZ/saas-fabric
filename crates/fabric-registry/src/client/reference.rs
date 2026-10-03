//! What a caller may ask to resolve: a tag, or a `sha256` digest.

use fabric_platform_management::RegistryError;

use crate::client::digest::sha256;

/// What a caller asked to resolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reference<'a> {
    /// A tag, in the distribution specification's grammar.
    Tag(&'a str),

    /// A `sha256` digest.
    Digest(&'a str),
}

/// Reads a reference as a tag or a `sha256` digest.
///
/// A tag cannot contain `:`, so anything that does is a digest, and must be
/// a `sha256` one. A tag is checked against the grammar because it becomes
/// part of a URL path: a `/`, `?` or `#` in one would ask for something else.
///
/// # Errors
///
/// [`RegistryError::Refused`] for a digest in another algorithm or form, or
/// for neither a tag nor a digest. The reference itself is not repeated in
/// the message: it failed the only grammar that makes it safe to show.
pub(super) fn reference(text: &str) -> Result<Reference<'_>, RegistryError> {
    if text.contains(':') {
        return sha256(text, "a reference").map(Reference::Digest);
    }

    let mut bytes = text.bytes();
    let first_ok = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    let rest_ok = bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'));

    if first_ok && rest_ok && text.len() <= 128 {
        return Ok(Reference::Tag(text));
    }

    Err(RegistryError::Refused {
        detail: "a reference was asked for that is neither a tag nor a sha256 digest".to_owned(),
    })
}
