//! Turns held bytes, already read off disk by [`super::held`], into typed
//! documents.
//!
//! Reading bytes and deciding what they mean are different concerns:
//! `held` answers "does a file exist, and what raw bytes or manifest does it
//! hold"; this module answers "what does an array-shaped document's held
//! bytes actually represent".

use serde::de::DeserializeOwned;

use super::held::unreadable;
use crate::{DocumentKind, DocumentManifest, PublicationError, TenantBindingDocument};

/// Parses a held payload as a JSON array of `T`, treating an absent payload
/// as an empty set.
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
///
/// # Why the held tenants document does not use this function
///
/// This function cannot tell "never published" apart from "published, then
/// the payload was lost while its manifest survived" — both look like
/// `payload: None` here, because it is never told whether a manifest exists.
/// That collapse is harmless for data sources: their held payload feeds only
/// their own emptying guard, and a lost payload's own verdict is an
/// unconditional `Write` regardless (ADR 0018 part 6's "manifest held,
/// payload absent" row), so any publication rewrites it. The held *tenants*
/// document also gates the retirement guard on a *different* document (data
/// sources), where a wrongly-assumed "empty" could let a live DataSource be
/// retired out from under a tenant that never stopped using it, with no
/// later write to undo the mistake. [`parse_held_tenants`] is the
/// fail-closed sibling that exists for that reason.
pub(super) fn parse_documents<T: DeserializeOwned>(
    payload: Option<&[u8]>,
    document: DocumentKind,
) -> Result<Vec<T>, PublicationError> {
    match payload {
        None => Ok(Vec::new()),
        Some(bytes) => serde_json::from_slice(bytes).map_err(|error| unreadable(document, error)),
    }
}

/// Parses the held *tenants* document, distinguishing all three states a
/// held document can be in rather than collapsing two of them into "empty"
/// the way [`parse_documents`] safely can for other documents.
///
/// | Manifest | Payload | Result |
/// |---|---|---|
/// | absent | either | never published — `vec![]`, no constraint |
/// | present | present | parse and check |
/// | present | absent | held content unknown — refuse the whole publication |
///
/// A held manifest proves something was published; an absent payload means
/// that content is lost, not that nothing was ever published. See
/// [`PublicationError::HeldPayloadLost`] for why guessing "empty" is unsafe
/// here specifically, and for the operator's way out.
///
/// # Errors
///
/// [`PublicationError::Unreadable`] if the payload does not parse.
/// [`PublicationError::HeldPayloadLost`] if the manifest is held but the
/// payload is gone.
pub(super) fn parse_held_tenants(
    manifest: Option<&DocumentManifest>,
    payload: Option<&[u8]>,
    document: DocumentKind,
) -> Result<Vec<TenantBindingDocument>, PublicationError> {
    match (manifest, payload) {
        (None, _) => Ok(Vec::new()),
        (Some(_), Some(bytes)) => serde_json::from_slice(bytes).map_err(|error| unreadable(document, error)),
        (Some(_), None) => Err(PublicationError::HeldPayloadLost { document }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DocumentRevision;

    #[test]
    fn an_absent_payload_parses_as_an_empty_set() {
        let parsed: Vec<TenantBindingDocument> = parse_documents(None, DocumentKind::DataSources).unwrap();

        assert!(parsed.is_empty());
    }

    #[test]
    fn unparseable_bytes_are_refused_as_unreadable() {
        let error = parse_documents::<TenantBindingDocument>(Some(b"not json"), DocumentKind::DataSources)
            .unwrap_err();

        assert!(matches!(error, PublicationError::Unreadable { .. }));
    }

    #[test]
    fn no_manifest_for_tenants_is_never_published_regardless_of_any_payload() {
        assert!(parse_held_tenants(None, None, DocumentKind::Tenants)
            .unwrap()
            .is_empty());
        assert!(parse_held_tenants(None, Some(b"[]"), DocumentKind::Tenants)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn a_tenants_manifest_without_a_payload_is_refused_as_held_payload_lost() {
        let manifest = DocumentManifest::new(DocumentKind::Tenants, DocumentRevision::new(1));

        let error = parse_held_tenants(Some(&manifest), None, DocumentKind::Tenants).unwrap_err();

        assert!(matches!(
            error,
            PublicationError::HeldPayloadLost {
                document: DocumentKind::Tenants
            }
        ));
    }

    #[test]
    fn a_tenants_manifest_with_a_payload_parses_it() {
        let manifest = DocumentManifest::new(DocumentKind::Tenants, DocumentRevision::new(1));

        let parsed = parse_held_tenants(Some(&manifest), Some(b"[]"), DocumentKind::Tenants).unwrap();

        assert!(parsed.is_empty());
    }
}
