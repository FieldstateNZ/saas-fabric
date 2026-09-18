//! Why a declared data source is refused, in the platform's own words.

use fabric_runtime_publication::PlacementClassDocument;

/// Why a declaration violates one of ADR 0023 part 1's placement rules.
///
/// Each variant's Display is the message an operator reads. It names the
/// rule that was broken and nothing about where the declaration would have
/// been stored -- these are refused before a byte reaches Git, so there is
/// no file to point at yet.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DataSourceRule {
    /// A shared data source must isolate tenants by a discriminator
    /// column (ADR 0006); nothing else can tell one tenant's rows from
    /// another's on it.
    #[error("a shared data source needs a discriminator column")]
    SharedNeedsDiscriminator,

    /// A discriminator column only means something on a shared data
    /// source. Declaring one anywhere else claims an isolation the
    /// placement class does not serve, and is refused rather than ignored.
    #[error(
        "a discriminator column only applies to a shared data source, not a {} one",
        placement_word(*placement)
    )]
    DiscriminatorOnlyWhenShared {
        /// The placement class the discriminator was declared on. Held as
        /// the wire's own enum rather than a pre-rendered word, so the
        /// platform's phrasing lives in one place -- this Display -- and
        /// not wherever the rule happens to be constructed.
        placement: PlacementClassDocument,
    },

    /// A pool setting of zero is not "unlimited" or "use the default" -- it
    /// is a connector that can never open a connection.
    #[error("{} must be greater than zero", field.operator_words())]
    ZeroPool {
        /// Which pool setting was zero.
        field: PoolField,
    },

    /// A label with an empty key or value is not a fact about the data
    /// source; it is a form left half filled in.
    #[error("a label cannot have an empty key or value")]
    EmptyLabel,

    /// A Secret connection selector carries a reference that is not a
    /// usable path: empty, over 512 bytes, or containing whitespace or a
    /// control character.
    ///
    /// Checked here rather than left to whatever eventually resolves it,
    /// because a malformed reference is not a credential failure to
    /// discover at request time -- it is a typo in a form, refused before
    /// it is ever believed to name anything.
    #[error(
        "a secret reference must be a non-empty path of at most 512 bytes, with no \
         whitespace or control characters"
    )]
    MalformedSecretReference,

    /// The wire's third connection shape -- the connector's single default
    /// connection -- is not something an operator declares. A data source
    /// names the connection it uses; leaving that unstated is not a
    /// smaller declaration, it is a different question this platform does
    /// not answer here.
    #[error("a connection must be a name the connector holds or a reference to a secret")]
    ConnectionKindNotDeclarable,
}

/// The word an operator reads for a placement class, in the platform's own
/// words rather than the wire's `snake_case` spelling.
fn placement_word(placement: PlacementClassDocument) -> &'static str {
    match placement {
        PlacementClassDocument::Shared => "shared",
        PlacementClassDocument::Dedicated => "dedicated",
        PlacementClassDocument::HighAvailability => "high availability",
        PlacementClassDocument::Regulated => "regulated",
        PlacementClassDocument::Development => "development",
        PlacementClassDocument::Ephemeral => "ephemeral",
    }
}

/// Which pool setting `DataSourceRule::ZeroPool` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolField {
    /// `pool.max_connections`.
    MaxConnections,
    /// `pool.idle_timeout_seconds`.
    IdleTimeoutSeconds,
    /// `pool.acquire_timeout_seconds`.
    AcquireTimeoutSeconds,
}

impl PoolField {
    /// The words an operator reads for this field, in the platform's own
    /// vocabulary rather than the wire's `snake_case` spelling.
    const fn operator_words(self) -> &'static str {
        match self {
            Self::MaxConnections => "the pool's maximum connections",
            Self::IdleTimeoutSeconds => "the pool's idle timeout",
            Self::AcquireTimeoutSeconds => "the pool's acquire timeout",
        }
    }
}
