//! A key a source declared, and how often it declared it.

/// One key of a source, kept with its count.
///
/// The count is kept because a duplicate is not a value to pick between: a
/// source saying `repoURL` twice may or may not be the one the pin names,
/// depending on which line is read, and one saying `targetRevision` twice has
/// two revisions with nothing to say which was meant. Both are refused, and
/// counting is how they are noticed.
pub(super) struct Seen<T> {
    /// The first one declared, kept only so a lone declaration can be read.
    pub(super) first: Option<T>,
    /// How many were declared.
    pub(super) count: usize,
}

impl<T> Seen<T> {
    /// Records one declaration.
    pub(super) fn record(&mut self, value: T) {
        self.count += 1;
        self.first.get_or_insert(value);
    }
}

impl<T> Default for Seen<T> {
    /// Nothing declared yet. A derived `Default` would demand one of `T` too,
    /// which no key of a source has.
    fn default() -> Self {
        Self {
            first: None,
            count: 0,
        }
    }
}
