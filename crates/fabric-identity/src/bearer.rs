//! Pulling the bearer token out of the `Authorization` header.

use http::HeaderMap;

use crate::IdentityError;

/// Extracts the bearer token from an `Authorization` header.
///
/// The scheme match is case-insensitive because RFC 7235 says the scheme is
/// case-insensitive, and real clients send `bearer`, `Bearer`, and `BEARER`.
/// The token itself is returned untrimmed apart from the single delimiting
/// space — a token with stray whitespace is malformed and should fail
/// verification rather than be silently repaired.
///
/// # Errors
///
/// - [`IdentityError::MissingAuthorization`] when the header is absent or is
///   not valid ASCII.
/// - [`IdentityError::NotBearer`] when the header is present but is not a
///   non-empty bearer credential.
pub(crate) fn extract_bearer(headers: &HeaderMap) -> Result<&str, IdentityError> {
    let header = headers
        .get(http::header::AUTHORIZATION)
        .ok_or(IdentityError::MissingAuthorization)?
        .to_str()
        .map_err(|_| IdentityError::MissingAuthorization)?;

    let (scheme, token) = header.split_once(' ').ok_or(IdentityError::NotBearer)?;

    if !scheme.eq_ignore_ascii_case("bearer") || token.is_empty() {
        return Err(IdentityError::NotBearer);
    }

    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers_with(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::AUTHORIZATION, value.parse().unwrap());
        headers
    }

    #[test]
    fn extracts_a_bearer_token() {
        assert_eq!(
            extract_bearer(&headers_with("Bearer abc.def.ghi")).unwrap(),
            "abc.def.ghi"
        );
    }

    #[test]
    fn the_scheme_match_is_case_insensitive() {
        assert_eq!(extract_bearer(&headers_with("bearer token")).unwrap(), "token");
        assert_eq!(extract_bearer(&headers_with("BEARER token")).unwrap(), "token");
    }

    #[test]
    fn a_missing_header_is_reported_as_missing_not_as_a_bad_scheme() {
        assert_eq!(
            extract_bearer(&HeaderMap::new()).unwrap_err(),
            IdentityError::MissingAuthorization
        );
    }

    #[test]
    fn a_basic_credential_is_rejected() {
        assert_eq!(
            extract_bearer(&headers_with("Basic dXNlcjpwYXNz")).unwrap_err(),
            IdentityError::NotBearer
        );
    }

    #[test]
    fn an_empty_bearer_credential_is_rejected() {
        assert_eq!(
            extract_bearer(&headers_with("Bearer ")).unwrap_err(),
            IdentityError::NotBearer
        );
    }
}
