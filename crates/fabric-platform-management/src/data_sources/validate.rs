//! The rules in ADR 0023 part 1, "Validation is the control plane's".

use fabric_runtime_publication::{ConnectionSelectorDocument, PlacementClassDocument};

use crate::data_sources::declaration::DataSourceDeclaration;
use crate::data_sources::rule::{DataSourceRule, PoolField};

/// The longest a `Secret` reference may be.
///
/// Chosen to match the platform repository's own header comment style: a
/// round, generous bound rather than a measured one -- a reference is a
/// path, not a payload, and nothing legitimate approaches it.
const MAX_SECRET_REFERENCE_BYTES: usize = 512;

impl DataSourceDeclaration {
    /// Checks this declaration against every rule ADR 0023 part 1 states.
    ///
    /// Refusing here means the runtime never sees a combination it would
    /// refuse later -- the same refusal, moved earlier.
    ///
    /// # Errors
    ///
    /// The `DataSourceRule` that was broken.
    pub fn validate(&self) -> Result<(), DataSourceRule> {
        if self.placement == PlacementClassDocument::Shared {
            if self.discriminator.is_none() {
                return Err(DataSourceRule::SharedNeedsDiscriminator);
            }
        } else if self.discriminator.is_some() {
            return Err(DataSourceRule::DiscriminatorOnlyWhenShared {
                placement: self.placement,
            });
        }

        match &self.connection {
            ConnectionSelectorDocument::Default {} => {
                return Err(DataSourceRule::ConnectionKindNotDeclarable);
            }
            ConnectionSelectorDocument::Secret { reference } => {
                let malformed = reference.is_empty()
                    || reference.len() > MAX_SECRET_REFERENCE_BYTES
                    || reference
                        .chars()
                        .any(|character| character.is_whitespace() || character.is_control());

                if malformed {
                    return Err(DataSourceRule::MalformedSecretReference);
                }
            }
            ConnectionSelectorDocument::Named { .. } => {}
        }

        if self.pool.max_connections == 0 {
            return Err(DataSourceRule::ZeroPool {
                field: PoolField::MaxConnections,
            });
        }
        if self.pool.idle_timeout_seconds == 0 {
            return Err(DataSourceRule::ZeroPool {
                field: PoolField::IdleTimeoutSeconds,
            });
        }
        if self.pool.acquire_timeout_seconds == 0 {
            return Err(DataSourceRule::ZeroPool {
                field: PoolField::AcquireTimeoutSeconds,
            });
        }

        for (key, value) in &self.labels {
            if key.is_empty() || value.is_empty() {
                return Err(DataSourceRule::EmptyLabel);
            }
        }

        Ok(())
    }
}
