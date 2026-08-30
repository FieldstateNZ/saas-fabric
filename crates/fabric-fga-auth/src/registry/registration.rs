//! One issuer Fabric is willing to trust, and everything that follows from it.

use jsonwebtoken::Algorithm;
use serde::Deserialize;

/// How stale a cached key set may be before verification stops trusting it.
///
/// A default rather than a required field: every deployment wants a bound, and
/// almost none has an opinion about the number. Twelve hours is long enough
/// that an ordinary provider outage never reaches it and short enough that a
/// key removed during a long one does not stay trusted for days.
const DEFAULT_MAX_KEY_AGE_SECONDS: u64 = 43_200;

/// A registration: an issuer, and what Fabric knows about it that a token
/// cannot say for itself.
///
/// # Why `jwks_uri` is here rather than discovered
///
/// The browser is sent to a public issuer; this process reads keys from
/// wherever it can actually reach — usually a cluster-local address. Splitting
/// the two is what lets an in-cluster verifier serve a public issuer without
/// hairpinning through the public route.
///
/// It is also why this does not reintroduce the request-forgery risk that
/// makes other implementations refuse private addresses: this URL comes from
/// Fabric's own configuration and is never taken from a claim. Nothing an
/// attacker controls selects a URL to fetch.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuerRegistration {
    /// The tenant this issuer's users belong to.
    ///
    /// The canonical identity, and the realm half of every principal minted
    /// from this issuer. It is **not** derived from the issuer URL: a URL is a
    /// claim until it has been matched against this registry, and a realm a
    /// caller could influence is worse than no realm at all.
    pub tenant: String,

    /// The value a token's `iss` must equal exactly.
    ///
    /// Exact match, not prefix and not pattern. An issuer that matches loosely
    /// is an issuer somebody else can look like.
    pub issuer: String,

    /// The audience a token must carry.
    pub audience: String,

    /// Where this process reads the issuer's signing keys.
    pub jwks_uri: String,

    /// The signature algorithms permitted for this issuer.
    ///
    /// Pinned per issuer, because the token header must not decide what
    /// cryptography is acceptable. Refusing `alg: none` is not enough on its
    /// own: anything outside this list is refused even where the library would
    /// happily verify it.
    pub algorithms: Vec<Algorithm>,

    /// The authorization store that answers for this tenant.
    pub store: String,

    /// How stale this issuer's cached keys may become.
    #[serde(default = "default_max_key_age")]
    pub max_key_age_seconds: u64,
}

/// The default staleness bound, as a function so `serde` can name it.
const fn default_max_key_age() -> u64 {
    DEFAULT_MAX_KEY_AGE_SECONDS
}

impl IssuerRegistration {
    /// Whether this registration permits an algorithm.
    #[must_use]
    pub fn permits(&self, algorithm: Algorithm) -> bool {
        self.algorithms.contains(&algorithm)
    }
}
