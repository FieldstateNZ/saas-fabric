//! Reading the token out of an `Authorization` header.

use http::HeaderMap;

/// The bearer token, if one was presented in the standard form.
///
/// The scheme is matched case-insensitively because RFC 7235 says it is
/// case-insensitive, and a client sending `bearer` is not making a mistake
/// worth refusing an operator over.
pub(super) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(http::header::AUTHORIZATION)?.to_str().ok()?;
    let token = value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))?;
    let token = token.trim();

    (!token.is_empty()).then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presenting(value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_str(value).expect("a legal header value"),
        );
        headers
    }

    #[test]
    fn reads_a_token_in_either_case_of_the_scheme() {
        assert_eq!(bearer(&presenting("Bearer abc.def.ghi")), Some("abc.def.ghi"));
        assert_eq!(bearer(&presenting("bearer abc.def.ghi")), Some("abc.def.ghi"));
    }

    #[test]
    fn ignores_anything_that_is_not_a_bearer() {
        assert_eq!(bearer(&presenting("Basic dXNlcjpwYXNz")), None);
        assert_eq!(bearer(&HeaderMap::new()), None);
    }

    #[test]
    fn a_scheme_with_no_token_presents_nothing() {
        assert_eq!(bearer(&presenting("Bearer")), None);
        assert_eq!(bearer(&presenting("Bearer    ")), None);
    }
}
