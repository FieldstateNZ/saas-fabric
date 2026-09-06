//! Reading a `v1` document into the `v2` shape, and refusing a `v2` document
//! that still carries `v1`'s field.
//!
//! Both run *before* the typed deserialisation, for the reason `parse` already
//! records about the document kind: a message about a missing field sends an
//! operator looking for something their document was never supposed to have,
//! where naming the replacement points them at the actual problem.
//!
//! Over the 120-line advisory threshold. The reason is that this is one
//! traversal of one document with one refusal beside it: the judgement it
//! makes — which strategy a flat list reads as, and when it refuses to guess —
//! is already split into `strategy`, and what is left is the walk, the two
//! keys it writes, and the accessors that reach `spec.identity.clients`.
//! Splitting the walk from the keys it writes would put half a rewrite in each
//! file.

mod strategy;

use serde_norway::Value;

use crate::DesiredStateError;

/// The dotted path these refusals name.
const FIELD: &str = "spec.identity.clients[].redirectUris";

/// What replaced it.
const REPLACEMENT: &str = "redirect, carrying a strategy and its uris";

/// Refuses a `v2` document that still carries `v1`'s callback list.
///
/// Applies to `v2` only. In a `v1` document `redirectUris` is not a mistake,
/// it is the schema.
///
/// # Errors
///
/// Returns [`DesiredStateError::Migration`] naming `redirect` and `strategy`.
pub(super) fn reject_replaced_field(raw: &Value) -> Result<(), DesiredStateError> {
    let carries_it =
        clients(raw).is_some_and(|clients| clients.iter().any(|client| client.get("redirectUris").is_some()));

    if !carries_it {
        return Ok(());
    }

    Err(DesiredStateError::Migration {
        field: FIELD,
        replacement: REPLACEMENT,
        detail: "a v2 document states which kind of callback each client is entitled to, so its \
                 callbacks live under redirect with a strategy beside them"
            .to_owned(),
    })
}

/// Rewrites a `v1` document's clients into the `v2` shape, in a copy.
///
/// The copy is what the typed view is derived from; the document itself is
/// left exactly as it was stored. Nothing reinterprets a document at rest —
/// only an operator's own edit moves one forward, which happens in
/// `render::with_identity`.
///
/// # Errors
///
/// Returns [`DesiredStateError::Migration`] for a client whose callbacks do
/// not all agree on one kind, or that already carries a `redirect` key, and
/// [`DesiredStateError::MissingField`] for one carrying neither.
pub(super) fn to_v2(mut raw: Value) -> Result<Value, DesiredStateError> {
    let Some(clients) = clients_mut(&mut raw) else {
        return Ok(raw);
    };

    for client in clients {
        migrate_client(client)?;
    }

    Ok(raw)
}

/// Rewrites one client's `redirectUris` into a `pkce` and a `redirect` block.
fn migrate_client(client: &mut Value) -> Result<(), DesiredStateError> {
    let Some(mapping) = client.as_mapping_mut() else {
        return Ok(());
    };

    if mapping.contains_key("redirect") {
        return Err(DesiredStateError::Migration {
            field: "spec.identity.clients[].redirect",
            replacement: REPLACEMENT,
            detail: "a v1 document has no redirect block; a document that carries one is a v2 \
                     document mislabelled as v1, and relabelling it is the operator's call"
                .to_owned(),
        });
    }

    let uris = mapping
        .remove("redirectUris")
        .ok_or(DesiredStateError::MissingField { field: FIELD })?;

    let strategy = strategy::of(&uris)?;

    // `s256` for every migrated client, which is the deliberate runtime break:
    // the next sweep requires a proof key of clients that never had one.
    mapping.insert(Value::String("pkce".to_owned()), Value::String("s256".to_owned()));
    mapping.insert(
        Value::String("redirect".to_owned()),
        redirect_block(strategy, uris),
    );

    Ok(())
}

/// The `redirect` mapping a migrated client gets.
fn redirect_block(strategy: &'static str, uris: Value) -> Value {
    let mut redirect = serde_norway::Mapping::new();
    redirect.insert(
        Value::String("strategy".to_owned()),
        Value::String(strategy.to_owned()),
    );
    redirect.insert(Value::String("uris".to_owned()), uris);

    Value::Mapping(redirect)
}

/// The declared application clients, if the document has any.
fn clients(raw: &Value) -> Option<&Vec<Value>> {
    raw.get("spec")?.get("identity")?.get("clients")?.as_sequence()
}

/// The declared application clients, to be rewritten in place.
fn clients_mut(raw: &mut Value) -> Option<&mut Vec<Value>> {
    raw.as_mapping_mut()?
        .get_mut("spec")?
        .as_mapping_mut()?
        .get_mut("identity")?
        .as_mapping_mut()?
        .get_mut("clients")?
        .as_sequence_mut()
}
