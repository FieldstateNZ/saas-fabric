//! Consuming an operator identity the network boundary established.

use std::collections::BTreeSet;

use http::{HeaderMap, HeaderName};

use crate::logging;
use crate::operator::{Operator, OperatorAuthError, OperatorAuthenticator};

/// Reads the operator from a header set by the operator-plane proxy.
///
/// # The allowlist is not optional
///
/// Being on the operator network establishes *who someone is*; it does not
/// establish that they administer this platform. The allowlist is the second
/// half, and it may not be empty — a configuration that omitted it would
/// authorise every identity the proxy can authenticate, which on a tailnet is
/// a larger set than anybody intends.
///
/// # Case, and why it is folded here but nowhere else
///
/// Subjects are email-shaped and identity providers are inconsistent about
/// case, so a comparison that treated `Brett@example.com` and
/// `brett@example.com` as different people would lock out the operator whose
/// provider changed its mind. Folding is safe here — this is a comparison
/// against a fixed list, not a value being interpolated into anything — and it
/// is exactly the reason [`TenantId`](fabric_core::TenantId) refuses to fold:
/// that one *is* interpolated.
pub struct TrustedHeaderOperators {
    /// The header the proxy states the operator's identity in.
    header: HeaderName,

    /// The operators permitted to administer this platform, lowercased.
    permitted: BTreeSet<String>,
}

impl TrustedHeaderOperators {
    /// Builds an authenticator over a header and an allowlist.
    ///
    /// # Errors
    ///
    /// Returns a message if the header name is not a legal HTTP header, or if
    /// the allowlist is empty — see this type's documentation for why an empty
    /// one is refused rather than treated as "allow everyone" or "allow
    /// nobody".
    pub fn new(header: &str, permitted: &[String]) -> Result<Self, String> {
        let header = HeaderName::try_from(header)
            .map_err(|error| format!("operator: {header} is not a valid header name: {error}"))?;

        let permitted: BTreeSet<String> = permitted
            .iter()
            .map(|subject| subject.trim().to_lowercase())
            .filter(|subject| !subject.is_empty())
            .collect();

        if permitted.is_empty() {
            return Err(
                "operator: the allowlist is empty, so no operator could administer the platform".to_owned(),
            );
        }

        Ok(Self { header, permitted })
    }
}

impl OperatorAuthenticator for TrustedHeaderOperators {
    fn authenticate(&self, headers: &HeaderMap) -> Result<Operator, OperatorAuthError> {
        let presented = headers
            .get(&self.header)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|subject| !subject.is_empty())
            .ok_or(OperatorAuthError::Missing)?;

        if !self.permitted.contains(&presented.to_lowercase()) {
            logging::operator_refused(self.header.as_str());
            return Err(OperatorAuthError::NotAnOperator);
        }

        Ok(Operator::new(presented))
    }

    fn describe(&self) -> String {
        format!(
            "trusted operator header {} ({} permitted)",
            self.header,
            self.permitted.len()
        )
    }
}
