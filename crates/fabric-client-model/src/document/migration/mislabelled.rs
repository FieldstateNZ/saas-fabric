//! Refusing a `v1` client that carries a key only `v2` has.
//!
//! Its own file because it is a different judgement from the one beside it.
//! The migrator's job is to read a `v1` client and produce the `v2` shape; the
//! judgement here is that this client is *not* a `v1` client at all. Both keys
//! it looks for were added by `v2`, so a document declaring `v1` and carrying
//! one is mislabelled, and relabelling it is the operator's call rather than
//! something a migrator may decide by overwriting.

use serde_norway::Mapping;

use crate::DesiredStateError;

/// What a `v1` document carrying a key only `v2` has needs instead.
///
/// Not the callback list's replacement text. Reusing that here would tell the
/// author of a stray `pkce` key that `pkce` "was replaced by redirect" — false
/// in both directions, and it would send them editing the one part of their
/// client that is fine. Nothing replaced these keys: the document is a `v2`
/// document wearing a `v1` label, and the label is what has to change.
const RELABEL: &str = "apiVersion: fabric.fieldstate.nz/v2, the schema this field belongs to";

/// Refuses one client that already carries a `redirect` block or a `pkce`
/// field.
///
/// `pkce` matters more than it looks. The migrator writes `pkce: s256`
/// unconditionally, so without this refusal a mislabelled document's own
/// value would be silently replaced — and a migrator that overwrites a
/// security setting to make its own rewrite succeed is the failure this file
/// exists to prevent.
///
/// # Errors
///
/// Returns [`DesiredStateError::Migration`] naming the key and the label that
/// would make it legal.
pub(super) fn reject(client: &Mapping) -> Result<(), DesiredStateError> {
    if client.contains_key("redirect") {
        return Err(mislabelled(
            "spec.identity.clients[].redirect",
            "a v1 document has no redirect block",
        ));
    }

    if client.contains_key("pkce") {
        return Err(mislabelled(
            "spec.identity.clients[].pkce",
            "a v1 document has no pkce field, and this migrator writes one",
        ));
    }

    Ok(())
}

/// The refusal both keys produce, differing only in what was found.
fn mislabelled(field: &'static str, found: &str) -> DesiredStateError {
    DesiredStateError::Migration {
        field,
        replacement: RELABEL,
        detail: format!(
            "{found}; a document that carries one is a v2 document mislabelled as v1, and \
             relabelling it is the operator's call, not a value this migrator may silently \
             overwrite"
        ),
    }
}
