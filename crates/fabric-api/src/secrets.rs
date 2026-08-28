//! An environment-backed secret resolver.

use async_trait::async_trait;
use fabric_connector::{ConnectorError, ResolvedSecret, SecretRef, SecretResolver};

/// Resolves secret references from environment variables.
///
/// A reference like `tenant/acme/data-primary` becomes
/// `FABRIC_SECRET_TENANT_ACME_DATA_PRIMARY`.
///
/// # What this is for
///
/// Development, and deployments where a secrets operator already projects
/// values into the pod's environment — which covers the External Secrets
/// Operator and the Vault agent injector, so it is not only a toy.
///
/// What it is *not* is a client for a secret store. §21 lists Azure Key Vault,
/// AWS Secrets Manager, HashiCorp Vault, and Kubernetes Secrets as things the
/// runtime may resolve against, and each of those is its own implementation of
/// this same trait. That is exactly why `SecretResolver` is a trait: the
/// physical secret location is a deployment concern, and no code above it can
/// observe which one is in use.
///
/// A real store client should cache. This one is reading process environment,
/// so there is nothing to cache.
pub struct EnvSecretResolver;

impl EnvSecretResolver {
    /// The prefix applied to every derived variable name.
    ///
    /// Visible to the crate so `config::env_namespace` can assert this
    /// namespace stays outside the settings one — the two shared a prefix
    /// once, and supplying a secret aborted startup.
    pub(crate) const PREFIX: &'static str = "FABRIC_SECRET_";

    /// Converts a reference into an environment variable name.
    ///
    /// Every character outside `A-Z`, `0-9` becomes an underscore, so the
    /// mapping is total and predictable. It is not injective — `a/b` and `a-b`
    /// collide — which is acceptable for a development resolver and is called
    /// out here so nobody discovers it by surprise.
    fn variable_name(reference: &SecretRef) -> String {
        let sanitised: String = reference
            .as_str()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect();

        format!("{}{sanitised}", Self::PREFIX)
    }
}

#[async_trait]
impl SecretResolver for EnvSecretResolver {
    async fn resolve(&self, reference: &SecretRef) -> Result<ResolvedSecret, ConnectorError> {
        let name = Self::variable_name(reference);

        // The variable *name* is safe to report; the value is not, and is not
        // touched on the error path.
        std::env::var(&name)
            .map(ResolvedSecret::new)
            .map_err(|_| ConnectorError::SecretUnavailable {
                reference: reference.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_an_environment_variable_name_from_a_reference() {
        let reference = SecretRef::new("tenant/acme/data-primary");

        assert_eq!(
            EnvSecretResolver::variable_name(&reference),
            "FABRIC_SECRET_TENANT_ACME_DATA_PRIMARY"
        );
    }

    #[tokio::test]
    async fn an_unset_variable_fails_closed() {
        let reference = SecretRef::new("tenant/nonexistent/thing");

        let error = EnvSecretResolver.resolve(&reference).await.unwrap_err();

        assert!(matches!(error, ConnectorError::SecretUnavailable { .. }));
    }

    #[tokio::test]
    async fn the_error_names_the_reference_and_not_a_value() {
        let reference = SecretRef::new("tenant/acme/data-primary");

        let message = EnvSecretResolver
            .resolve(&reference)
            .await
            .unwrap_err()
            .to_string();

        assert!(message.contains("tenant/acme/data-primary"));
    }
}
