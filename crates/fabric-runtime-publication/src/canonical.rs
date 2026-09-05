//! The one formatting rule every published document shares.

use serde::Serialize;

/// Renders `value` as the platform's canonical JSON: two-space indentation,
/// UTF-8, and a trailing newline.
///
/// This is load-bearing rather than cosmetic. The publisher's own
/// divergent-payload guard (`crate::verdict::verdict`) refuses a
/// same-revision publication by **comparing bytes** against what is already
/// on disk. Two
/// callers publishing an identical snapshot must therefore produce
/// byte-identical output regardless of field order or whitespace, or the
/// guard would see a difference that means nothing — this function is the
/// one place that formatting decision is made, so it cannot drift between
/// the three documents.
///
/// # Errors
///
/// Returns [`serde_json::Error`] only if `value`'s own `Serialize`
/// implementation fails, which none of this crate's document types ever do.
pub(crate) fn to_canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}
