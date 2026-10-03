//! How much of each answer this client reads, and how far it follows one.
//!
//! # Why every read has its own bound
//!
//! Every body is bounded before it is parsed (ADR 0026 section 4): an
//! unbounded read of a remote document is an unbounded allocation decided by
//! somebody else. Each bound is far past anything a registry this platform
//! reads sends for that kind of answer, and still a bound.

/// A manifest or an index, including the referrers tag schema's index.
pub(super) const MANIFEST: usize = 4 * 1024 * 1024;

/// One page of a referrers listing, and a referrers `404`'s error body.
pub(super) const REFERRERS: usize = 1024 * 1024;

/// A token response: a bearer and a little metadata.
pub(super) const TOKEN: usize = 16 * 1024;

/// One page of a tag listing.
pub(super) const TAG_PAGE: usize = 1024 * 1024;

/// An image's config blob, which holds its labels.
pub(super) const CONFIG: usize = 1024 * 1024;

/// How many referrers pages are followed before giving up. Running past it
/// is an error, never a silently shortened list.
pub(super) const REFERRER_PAGES: usize = 10;

/// How many candidate component descriptors are fetched one by one. More
/// than this is already more than one listed, and answered as
/// [`Several`](fabric_platform_management::Attached::Several) without
/// fetching any — and so with no digests, none of them computed here.
pub(super) const CANDIDATES: usize = 16;
