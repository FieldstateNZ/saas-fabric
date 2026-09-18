//! Turns held bytes, already read off disk by [`super::held`], into typed
//! documents.
//!
//! Reading bytes and deciding what they mean are different concerns:
//! `held` answers "does a file exist, and what raw bytes or manifest does it
//! hold"; this module answers "what does an array-shaped document's held
//! bytes actually represent".

use serde::de::DeserializeOwned;

use crate::{DocumentKind, DocumentManifest, PublicationError};

/// Held bytes that will not parse are a read failure of that document.
pub(crate) fn unreadable(document: DocumentKind, cause: serde_json::Error) -> PublicationError {
    PublicationError::Unreadable {
        document,
        cause: Box::new(cause),
    }
}

/// Parses a payload as a JSON array of `T`, treating an absent payload as an
/// empty set.
///
/// Safe to call directly only when there is no manifest to consult at all —
/// the catalogue document, whose held state is only ever byte-compared
/// inside `verdict` and never parsed for a guard, so it has no caller of
/// [`parse_held_documents`]. For tenants and data sources, go through that
/// function instead: with a manifest in hand, an absent payload can mean
/// "never published" or "lost", and those are not the same thing.
///
/// # Absent versus unparseable
///
/// Absent means nothing has been published yet: there is no manifest and no
/// payload, so there is nothing this parse could be wrong about. An empty
/// set is the correct and safe answer.
///
/// Unparseable is different, and is refused rather than guessed at: a held
/// file this producer wrote should always parse, so one that does not is
/// either corrupted or hand-edited into a state this code cannot vouch for.
/// This returns [`PublicationError::Unreadable`] and leaves the decision to
/// an operator before the next publication is attempted.
pub(crate) fn parse_documents<T: DeserializeOwned>(
    payload: Option<&[u8]>,
    document: DocumentKind,
) -> Result<Vec<T>, PublicationError> {
    match payload {
        None => Ok(Vec::new()),
        Some(bytes) => serde_json::from_slice(bytes).map_err(|error| unreadable(document, error)),
    }
}

/// Parses a held document, distinguishing all three states a held document
/// can be in rather than collapsing two of them into "empty" the way
/// [`parse_documents`] alone would.
///
/// | Manifest | Payload | Result |
/// |---|---|---|
/// | absent | absent | never published — `vec![]` |
/// | absent | present | a payload shipped with no manifest beside it (the shipped `examples/*.json` today) is held content all the same, not "never published" — parsed and returned |
/// | present | present | parse and check |
/// | present | absent | held content unknown — refuse the whole publication |
///
/// A held manifest proves something was published; an absent payload means
/// that content is lost, not that nothing was ever published. See
/// [`PublicationError::HeldPayloadLost`] for why guessing "empty" is unsafe
/// here, and for the operator's way out.
///
/// Used for both the tenants and data-sources documents: each feeds its own
/// emptying guard, and the tenants document additionally gates the
/// data-sources retirement guard, so both need their *actual* held content
/// rather than a manifest-shaped guess at it. The catalogue document has no
/// such guard and so has no caller of this function — see
/// [`parse_documents`]'s own rustdoc.
///
/// # Errors
///
/// [`PublicationError::Unreadable`] if the payload does not parse.
/// [`PublicationError::HeldPayloadLost`] if the manifest is held but the
/// payload is gone.
pub(crate) fn parse_held_documents<T: DeserializeOwned>(
    manifest: Option<&DocumentManifest>,
    payload: Option<&[u8]>,
    document: DocumentKind,
) -> Result<Vec<T>, PublicationError> {
    match (manifest, payload) {
        (Some(_), None) => Err(PublicationError::HeldPayloadLost { document }),
        (_, payload) => parse_documents(payload, document),
    }
}

#[cfg(test)]
#[path = "parse_tests.rs"]
mod tests;
