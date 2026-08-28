//! Turning a contents entry into text and a revision.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use fabric_client_model::ClientRevision;
use fabric_control_plane::RepositoryError;

use crate::github::contents::StoredFile;
use crate::github::wire::ContentsEntry;

/// Decodes a file entry into text and a revision.
pub(super) fn decode(entry: ContentsEntry) -> Result<StoredFile, RepositoryError> {
    let encoded = entry.content.ok_or_else(|| RepositoryError::Unavailable {
        detail: "the repository returned a client entry with no content".to_owned(),
    })?;

    // The host wraps base64 at 60 columns, which the decoder will not accept.
    let stripped: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();

    let bytes = BASE64
        .decode(stripped)
        .map_err(|_| RepositoryError::Unavailable {
            detail: "the repository returned a client document that is not base64".to_owned(),
        })?;

    let text = String::from_utf8(bytes).map_err(|_| RepositoryError::Unavailable {
        detail: "the repository returned a client document that is not UTF-8".to_owned(),
    })?;

    let revision = ClientRevision::try_new(entry.sha).map_err(|error| RepositoryError::Unavailable {
        detail: format!("the repository reported an unusable revision: {error}"),
    })?;

    Ok(StoredFile { text, revision })
}
