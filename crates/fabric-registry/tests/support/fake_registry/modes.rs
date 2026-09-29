//! How the fake behaves, beyond what it holds.

use super::FakeRegistry;

impl FakeRegistry {
    /// Has the CDN redirect every blob once more, to another path on itself,
    /// before serving it: a chain whose last hop is on the same host as the
    /// hop before it.
    pub fn cdn_redirects_twice(&self) {
        self.locked().modes.cdn_second_hop = true;
    }

    /// Sends every body chunked, with no `Content-Length`, so a bound is met
    /// as the body arrives rather than against a declared length.
    pub fn unlengthed(&self) {
        self.locked().modes.unlengthed = true;
    }

    /// Answers a manifest `404 MANIFEST_UNKNOWN` unless the request accepted
    /// its media type, as CNCF distribution does.
    pub fn strict_accept(&self) {
        self.locked().modes.strict_accept = true;
    }
}
