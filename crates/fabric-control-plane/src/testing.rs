//! Seams for tests that drive the real control plane.
//!
//! # Why an operator can be minted here
//!
//! [`Operator`] has no constructor outside this crate, which
//! is what makes "did we check the operator?" a compile-time question. This
//! module is the one exception, and it is narrow on purpose: it hands back an
//! *authenticator*, not an operator, so the only way a test obtains one is
//! still by going through the extractor on a real request to the real router.
//!
//! The alternative was worse. Without it, every integration test would have to
//! publish a key set and mint tokens signed against it — proving that
//! `jsonwebtoken` verifies signatures, which
//! [`OidcOperators`](crate::OidcOperators)' own tests already do, at the cost of
//! making every other test about authentication.
//!
//! Nothing here is reachable from a deployment: `build_control_plane` takes
//! the authenticator as an `Option` that only tests populate, and the
//! configured posture is what builds it otherwise.

use std::sync::Arc;

use http::HeaderMap;

use crate::operator::{Operator, OperatorAuthError, OperatorAuthenticator, OperatorToken};

/// The header this authenticator reads.
///
/// It is not a posture and never reaches a deployment. It exists so that an
/// *unauthenticated* request stays expressible in a test: without it, every
/// request would be accepted and the tests asserting `401` would pass while
/// proving nothing.
pub const TEST_OPERATOR_HEADER: &str = "X-Test-Operator";

/// Accepts a request carrying [`TEST_OPERATOR_HEADER`], and refuses one that
/// does not.
pub struct AcceptingOperator {
    /// Who an accepted request is attributed to.
    subject: String,
}

impl AcceptingOperator {
    /// Accepts requests bearing the test header as `subject`.
    ///
    /// Returns the trait object rather than `Self` because that is the only
    /// shape a caller wants: there is nothing to do with a concrete one.
    #[must_use]
    pub fn accepting(subject: impl Into<String>) -> Arc<dyn OperatorAuthenticator> {
        Arc::new(Self {
            subject: subject.into(),
        })
    }
}

impl OperatorAuthenticator for AcceptingOperator {
    fn authenticate(&self, headers: &HeaderMap) -> Result<Operator, OperatorAuthError> {
        if !headers.contains_key(TEST_OPERATOR_HEADER) {
            return Err(OperatorAuthError::Missing);
        }

        Ok(Operator::new(
            &self.subject,
            OperatorToken::new("a-fixture-bearer"),
        ))
    }

    fn describe(&self) -> String {
        format!(
            "a test authenticator accepting requests bearing {TEST_OPERATOR_HEADER} as {}",
            self.subject
        )
    }
}
