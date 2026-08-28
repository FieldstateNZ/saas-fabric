//! One physical destination, as far as configuration alone can express it.

use fabric_connector::{ConnectionName, ConnectionSelector};

/// The destination a [`ConnectionSelector`] names, reduced to something two
/// DataSources can be compared on.
///
/// A private enum rather than a formatted string because a string key would
/// have to invent a separator no identifier could contain, and a collision here
/// means refusing a tenant that was fine.
///
/// # Why the secret variant does not hold the reference
///
/// The other two variants key on the value itself, because the runtime has no
/// mapping of its own applied to either. [`ConnectionSelector::Named`] names a
/// connection in the *connector's* configuration, and the connector owns that
/// namespace; folding two names together here would be inventing a hazard.
///
/// A [`SecretRef`](fabric_connector::SecretRef) is different, and the
/// difference is the whole reason this variant exists. The reference does not
/// reach a store as written — a resolver projects it into a physical namespace,
/// and that projection is not injective. `EnvSecretResolver`, the resolver this
/// workspace ships, maps every non-alphanumeric character to `_`, so
/// `vault/prod/customer-db-01` and `vault/prod/customer_db_01` are one
/// environment variable and therefore one connection string. Keyed on the raw
/// reference, those two read as two destinations: two tenants, `Exclusive` on
/// both sides, structural isolation permitted, and one database served to both
/// with no predicate and no error.
///
/// So the variant keys on
/// [`SecretRef::distinctness_key`](fabric_connector::SecretRef::distinctness_key),
/// which discards exactly what a projection discards — case, and the identity
/// of every non-alphanumeric character. Two references that differ by no more
/// than that become one destination here, and the tenants sharing it are
/// refused structural isolation by the rule that was already in place.
///
/// # Why this and not a validated `SecretRef`
///
/// A checked constructor rejecting collision-capable characters would put the
/// guarantee at the type, which is the shape this codebase usually reaches for.
/// [`SecretRef::distinctness_key`](fabric_connector::SecretRef::distinctness_key)
/// records why it loses here; the short form is that collision is a property of
/// a *pair*, so only a place that sees the whole set can decide it — and this
/// is that place.
///
/// # Why not refuse the colliding DataSources outright
///
/// Because it would take working tenants offline over a fault that does not
/// touch them. A collision makes *structural* isolation unenforceable and
/// leaves discriminator isolation exactly as safe as it was, so dropping the
/// DataSources would break every tenant on them to protect the subset that
/// needed protecting. Making them one destination refuses precisely the
/// bindings that are unsafe — the same trade
/// [`DataSource::validate`](crate::DataSource::validate) already argues for the
/// plain connection-name collision.
///
/// # The honest limit, unchanged
///
/// Two *genuinely different* references resolving to one credential — Vault
/// aliases, two store entries holding one connection string — still read as two
/// destinations. Seeing that needs the resolver, and §6 keeps that round trip
/// off the request path. What has changed is only that a collision manufactured
/// by a resolver's own mapping is no longer in the same category as that limit:
/// it is decidable from the snapshot, with no I/O at all.
#[derive(PartialEq, Eq, Hash)]
pub(super) enum Destination<'a> {
    /// The connector's single configured connection.
    Default,

    /// A connection the connector holds configuration for.
    Named(&'a ConnectionName),

    /// A connection built from a resolved credential, keyed on what survives a
    /// resolver's projection rather than on the reference as written.
    ///
    /// Owned, and the one allocation in deriving the fact. It is paid once per
    /// secret-backed DataSource when a snapshot installs, never on the request
    /// path, which is a straightforward trade against a cross-tenant leak.
    Secret(String),
}

/// Reduces a selector to the destination it names.
pub(super) fn destination(selector: &ConnectionSelector) -> Destination<'_> {
    match selector {
        ConnectionSelector::Default => Destination::Default,
        ConnectionSelector::Named { name } => Destination::Named(name),
        ConnectionSelector::Secret { reference } => Destination::Secret(reference.distinctness_key()),
    }
}
