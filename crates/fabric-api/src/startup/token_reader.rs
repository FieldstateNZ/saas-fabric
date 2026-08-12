//! Building the configured token reader.

use std::sync::Arc;

use fabric_core::SystemClock;
use fabric_identity::{TokenReader, TrustedIngressReader, ValidatingReader, VerificationKeys};

use crate::config::TokenConfig;

/// Builds the reader the configuration asks for.
///
/// Which reader runs is a security decision, so it is made here in the
/// composition root rather than inside the identity crate — a reader of this
/// file can see the deployed posture without going looking.
///
/// # Errors
///
/// Returns a message if a JWKS document cannot be read or parsed. Only the
/// defence-in-depth mode can fail: the canonical posture needs no key material.
pub(super) fn build(config: &TokenConfig) -> Result<Arc<dyn TokenReader>, String> {
    match config {
        // The canonical posture (§8, §9): the edge authenticated the caller and
        // the runtime consumes the identity it established. `build_identity`
        // records it at info — a correctly configured deployment is not a
        // problem to warn about on every start. See ADR 0002.
        TokenConfig::TrustedIngress => Ok(Arc::new(TrustedIngressReader::new(SystemClock::shared()))),

        TokenConfig::Validating {
            jwks_path,
            issuers,
            audiences,
        } => {
            let document = std::fs::read_to_string(jwks_path)
                .map_err(|error| format!("could not read JWKS from {}: {error}", jwks_path.display()))?;

            let mut reader = ValidatingReader::new(VerificationKeys::from_jwks_json(&document)?);

            if !issuers.is_empty() {
                reader = reader.with_issuers(issuers);
            }

            if !audiences.is_empty() {
                reader = reader.with_audiences(audiences);
            }

            Ok(Arc::new(reader))
        }
    }
}
