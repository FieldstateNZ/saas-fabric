//! Wiring for the identity domain.

use std::sync::Arc;

use crate::{logging, IdentityConfig, IdentityResolver, TokenReader};

/// Builds the resolver, then records the identity posture.
///
/// The token reader is passed in rather than chosen here. Which reader a
/// deployment runs is a security decision, and it should be legible in the
/// composition root next to everything else the process trusts — not selected
/// by a string in a config file three layers down.
///
/// Validation itself belongs to [`IdentityResolver::new`] — a resolver cannot
/// exist without a validated registry, as a property of the type — so this
/// function keeps only the logging.
///
/// # Errors
///
/// Returns a message if the configuration is invalid. Called before the server
/// binds, so a bad claim name stops the process rather than failing every
/// request at runtime with no obvious cause.
pub fn build_identity(
    config: IdentityConfig,
    reader: Arc<dyn TokenReader>,
) -> Result<Arc<IdentityResolver>, String> {
    let description = reader.describe();
    let tenant_claim = config.tenant_claim.clone();

    let resolver = IdentityResolver::new(config, reader)?;

    logging::reader_configured(description, &tenant_claim);

    Ok(Arc::new(resolver))
}
