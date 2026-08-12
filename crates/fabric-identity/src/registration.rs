//! Wiring for the identity domain.

use std::sync::Arc;

use crate::{logging, IdentityConfig, IdentityResolver, TokenReader};

/// Validates identity configuration and builds the resolver.
///
/// The token reader is passed in rather than chosen here. Which reader a
/// deployment runs is a security decision, and it should be legible in the
/// composition root next to everything else the process trusts — not selected
/// by a string in a config file three layers down.
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
    config.validate()?;

    logging::reader_configured(reader.describe(), &config.tenant_claim);

    Ok(Arc::new(IdentityResolver::new(config, reader)))
}
