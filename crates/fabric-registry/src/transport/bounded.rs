//! Reading a response body without letting its sender decide how much.

use fabric_platform_management::RegistryError;

use crate::errors::transport_failure;

/// Reads `response`'s body, refusing anything past `most` bytes.
///
/// # Why streamed, and why every body
///
/// An unbounded read of a remote document is an unbounded allocation decided
/// by somebody else, and `.json()` or `.bytes()` is exactly that. A declared
/// `Content-Length` past the bound is refused before a byte is read; one
/// that lied, or none at all, is caught as the body arrives, chunk by chunk,
/// rather than after it has all been held.
///
/// `operation` names what was being read, as every message in this crate
/// does, and is the only text a refusal carries: never the URL, never the
/// body.
///
/// # Errors
///
/// [`RegistryError::Refused`] past the bound; [`RegistryError::Unavailable`]
/// if the body could not be read to its end.
pub(crate) async fn bounded_body(
    mut response: reqwest::Response,
    most: usize,
    operation: &str,
) -> Result<Vec<u8>, RegistryError> {
    let too_large = || RegistryError::Refused {
        detail: format!("{operation} answered with more than {most} bytes"),
    };

    let declared = response
        .content_length()
        .and_then(|length| usize::try_from(length).ok());
    if response.content_length().is_some() && declared.is_none_or(|length| length > most) {
        return Err(too_large());
    }

    let mut body = Vec::with_capacity(declared.unwrap_or(0));

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| transport_failure(operation, &error))?
    {
        if body.len() + chunk.len() > most {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }

    Ok(body)
}
