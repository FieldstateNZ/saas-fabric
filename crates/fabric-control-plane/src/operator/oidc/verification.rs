//! Checking a token's signature and registered claims.
//!
//! Split from the posture itself because they answer different questions: the
//! posture decides *who counts as an operator*, and this decides *whether this
//! token is genuine*. A change to one is almost never a change to the other.

use jsonwebtoken::{decode, decode_header, Validation};

use super::claims::OperatorClaims;
use super::OidcOperators;
use crate::operator::OperatorAuthError;

impl OidcOperators {
    /// The rules every accepted token is checked against.
    ///
    /// Audience validation is switched off deliberately; see
    /// [`OperatorClaims::azp`](claims::OperatorClaims) for why the client is
    /// established from `azp` instead.
    pub(super) fn rules(&self) -> Validation {
        let mut validation = Validation::new(self.algorithm);
        validation.set_issuer(&[&self.issuer]);
        validation.validate_aud = false;
        validation.leeway = self.leeway;
        validation
    }

    /// Verifies the token and reads its claims, or says why it was refused.
    pub(super) fn verify(&self, token: &str) -> Result<OperatorClaims, OperatorAuthError> {
        let header = decode_header(token).map_err(|_| OperatorAuthError::NotAnOperator)?;
        let held = self.keys.current();
        let rules = self.rules();

        // Every candidate is tried rather than the first: a realm mid-rotation
        // publishes two keys, and a token signed by either is genuine.
        held.candidates(header.kid.as_deref())
            .into_iter()
            .find_map(|key| decode::<OperatorClaims>(token, key, &rules).ok())
            .map(|data| data.claims)
            .ok_or(OperatorAuthError::NotAnOperator)
    }
}
