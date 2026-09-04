//! Which generation a request was prepared against, as a token only the order mints.

/// Which generation a request was prepared against.
///
/// A token rather than a number: minted only by `Order::observed`, read back
/// only by `Order::settle`, and constructible nowhere else — so a transition
/// cannot be handed a generation somebody typed, and the one it is handed
/// came from a read of the order, which `stored.rs` makes the first read,
/// before the record and the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::git_integration::service) struct Generation(u64);

impl Generation {
    /// Only the order mints one.
    pub(super) const fn minted(value: u64) -> Self {
        Self(value)
    }
}
