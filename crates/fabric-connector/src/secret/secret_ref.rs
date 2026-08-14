//! A pointer to a credential, and the one thing that can be said about two of
//! them without asking a resolver.

use std::fmt;

/// A pointer to a credential, as it appears in a tenant's runtime binding.
///
/// Specification §21: secrets are never stored in Git tenant definitions, only
/// *referenced*. This is that reference — a logical path such as
/// `tenant/acme/data-primary`. What it resolves to (Azure Key Vault, AWS Secrets
/// Manager, HashiCorp Vault, a Kubernetes Secret) is a deployment concern that
/// no code above [`SecretResolver`](super::SecretResolver) can observe.
///
/// The reference itself is not sensitive and may appear in logs. The *value* it
/// resolves to may never (§29).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SecretRef(String);

impl SecretRef {
    /// Wraps a secret reference path.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self(path.into())
    }

    /// Borrows the reference path.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A conservative key for *"these two references might be one credential"*.
    ///
    /// Two references whose keys are **equal** must be treated as possibly
    /// naming one credential. Two whose keys differ are not thereby guaranteed
    /// to name different ones — see the limit at the bottom.
    ///
    /// # The hazard
    ///
    /// A `SecretRef` is a logical path; a resolver projects it into some
    /// physical namespace. Those namespaces are narrower than the path — an
    /// environment variable name is `[A-Z0-9_]`, a Kubernetes Secret key is
    /// `[-._a-zA-Z0-9]` — and a projection into a narrower alphabet cannot be
    /// injective. Distinct references therefore land on one credential.
    ///
    /// This workspace ships an example. `EnvSecretResolver` maps every
    /// character outside `[A-Za-z0-9]` to `_` and upper-cases the rest, so
    /// `vault/prod/customer-db-01` and `vault/prod/customer_db_01` are one
    /// environment variable and one connection string. A `-`/`_` slip between
    /// two generated secret paths is an ordinary reconciler mistake, and it is
    /// enough to put two tenants on one database while every layer above still
    /// counts two.
    ///
    /// So the key discards exactly what a projection discards: **case, and the
    /// identity of every non-alphanumeric character.** It is calibrated to the
    /// coarsest projection this workspace actually ships, not to a guess about
    /// resolvers in general — a resolver whose own mapping is coarser than this
    /// owes its callers a refusal, which is the contract stated on
    /// [`SecretResolver`](super::SecretResolver).
    ///
    /// # Why this rather than a validated character set on the type
    ///
    /// The house instinct — and usually the right one — is to make the illegal
    /// state unrepresentable: give `SecretRef` a checked constructor over an
    /// alphabet in which no two accepted values can collide. Rejected here, for
    /// three reasons that compound.
    ///
    /// First, the alphabet that survives is barely usable. Under a
    /// case-folding, punctuation-flattening projection, safety requires banning
    /// upper case *and* every separator but one — `vault/prod/customer-db-01`
    /// would be illegal over the hyphen, and so would the `tenant/acme/
    /// data-primary` used throughout this crate's own documentation and the
    /// shipped example's `initech-dedicated`.
    ///
    /// Second, it points the layering the wrong way. The rule would exist
    /// because of one resolver's mapping, but would be enforced on a type that
    /// every resolver shares — including stores whose own naming rules are
    /// unrelated and, in some cases, contradictory.
    ///
    /// Third, and decisively, it is the wrong shape of guarantee. A per-value
    /// constructor can only ever say "this one reference is well-formed".
    /// Collision is a property of a *pair*, and the pair is only visible where
    /// the whole set is: `fabric-tenant-runtime` derives it when a DataSource
    /// snapshot installs, with no I/O and no round trip.
    ///
    /// # What this cannot catch
    ///
    /// **Two genuinely different references that resolve to one credential.**
    /// Two Vault paths aliased to one secret, or two Key Vault names holding
    /// the same connection string, produce different keys here and read as two
    /// destinations. Nothing short of asking the resolver distinguishes them,
    /// and §6 keeps that round trip off the request path. That limit was true
    /// before this method existed and is unchanged by it; what has changed is
    /// that a collision *manufactured by a resolver's own mapping* is no longer
    /// in the same category, because it is decidable from the reference alone.
    ///
    /// # Not a telemetry label
    ///
    /// The key retains the reference's structure and is no safer to emit than
    /// the reference itself. Use
    /// [`ConnectionSelector::telemetry_label`](crate::ConnectionSelector::telemetry_label)
    /// for anything that ends up in a trace.
    #[must_use]
    pub fn distinctness_key(&self) -> String {
        self.0
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect()
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
