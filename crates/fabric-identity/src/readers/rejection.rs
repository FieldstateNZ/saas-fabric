//! Turning a `jsonwebtoken` failure into what the caller is told.

use jsonwebtoken::errors::{Error, ErrorKind};

use crate::IdentityError;

/// Maps a verification failure onto the rejection the caller sees.
///
/// Nearly every reason collapses into one opaque outcome. Telling an attacker
/// precisely which check failed narrows their search for free, so the specific
/// cause is logged and never returned.
///
/// The two validity-window outcomes are the exception, and are kept distinct
/// from that bucket and from each other for two reasons. Operators need to tell
/// a drifted clock from a replayed credential in the logs. And the canonical
/// [`TrustedIngressReader`](crate::TrustedIngressReader) reports exactly these
/// two for the same tokens — collapsing them here would leave the postures
/// agreeing that something was wrong while disagreeing about what.
///
/// This lives apart from the reader that calls it so `posture_parity_tests` can
/// hold the two postures against the *same* mapping. A parity test that
/// restated the mapping would keep passing while the real one drifted.
pub(crate) fn classify(error: &Error) -> IdentityError {
    match error.kind() {
        ErrorKind::ExpiredSignature => IdentityError::ExpiredToken,
        // `ImmatureSignature` is this library's name for a token whose `nbf`
        // has not yet arrived. Nothing about the signature is immature, and the
        // name is why this mapping is easy to get wrong.
        ErrorKind::ImmatureSignature => IdentityError::TokenNotYetValid,
        _ => IdentityError::UnverifiedToken,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expired_signature_is_reported_as_an_expired_token() {
        assert_eq!(
            classify(&Error::from(ErrorKind::ExpiredSignature)),
            IdentityError::ExpiredToken
        );
    }

    #[test]
    fn an_immature_signature_is_reported_as_a_token_that_is_not_yet_valid() {
        // The rename that matters: this library's `ImmatureSignature` is an
        // `nbf` failure, and must reach the caller as the same error the
        // canonical posture raises.
        assert_eq!(
            classify(&Error::from(ErrorKind::ImmatureSignature)),
            IdentityError::TokenNotYetValid
        );
    }

    #[test]
    fn every_other_reason_collapses_into_one_opaque_rejection() {
        for kind in [
            ErrorKind::InvalidSignature,
            ErrorKind::InvalidIssuer,
            ErrorKind::InvalidAudience,
            ErrorKind::MissingRequiredClaim("exp".to_owned()),
        ] {
            assert_eq!(classify(&Error::from(kind)), IdentityError::UnverifiedToken);
        }
    }
}
