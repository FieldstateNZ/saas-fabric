//! One entry of `spec.sources`, and the direct keys it declared.

use super::lines::Line;
use super::scalar::Scalar;
use super::seen::Seen;

/// One entry of `spec.sources`, as the walk found it.
///
/// # Only the entry's own keys count
///
/// A source is a mapping, and its identity is the `repoURL` and `chart` it
/// declares itself — not one appearing somewhere below it. A `helm:` block is
/// free to contain the words `chart` and `targetRevision` for reasons of its
/// own, and reading those as the source's would let a chart's configuration
/// decide which chart gets deployed, or move a revision nobody named. So every
/// key is measured against the one column this entry's keys sit at.
#[derive(Default)]
pub(super) struct Entry<'a> {
    /// The column the `- ` opening this entry sits at.
    at: usize,
    /// The column its own keys sit at, learned from the first of them.
    keys: Option<usize>,
    /// Its `repoURL`.
    repository: Seen<&'a str>,
    /// Its `chart`.
    chart: Seen<&'a str>,
    /// Its `targetRevision`: which line it is on, and that line read as a key.
    target: Seen<(usize, Scalar<'a>)>,
}

impl<'a> Entry<'a> {
    /// Opens an entry at a `- ` line, reading the key written beside the dash.
    pub(super) fn opened(index: usize, line: &Line<'a>) -> Self {
        let mut entry = Self {
            at: line.indent(),
            ..Self::default()
        };

        if let Some((column, first)) = line.after_dash() {
            entry.keys = Some(column);
            entry.record(index, line, first);
        }
        entry
    }

    /// Offers the entry a line found below it.
    pub(super) fn observe(&mut self, index: usize, line: &Line<'a>) {
        // A nested sequence's items are that list's, never this mapping's keys.
        if line.opens_entry() || line.indent() <= self.at {
            return;
        }

        if line.indent() == *self.keys.get_or_insert(line.indent()) {
            self.record(index, line, line.rest);
        }
    }

    /// Whether this is the source the manifest declared, by both halves of it.
    pub(super) fn names(&self, repository: &str, chart: &str) -> bool {
        self.repository.first == Some(repository) && self.chart.first == Some(chart)
    }

    /// Which key, if any, this source declared more than once.
    pub(super) fn said_twice(&self) -> Option<&'static str> {
        [
            ("repoURL", self.repository.count),
            ("chart", self.chart.count),
            ("targetRevision", self.target.count),
        ]
        .into_iter()
        .find_map(|(name, count)| (count > 1).then_some(name))
    }

    /// The line this source's own `targetRevision` is written on.
    pub(super) const fn target(&self) -> Option<&(usize, Scalar<'a>)> {
        self.target.first.as_ref()
    }

    /// Records one of the entry's own keys, ignoring all but the three it
    /// declares its identity and its revision with.
    fn record(&mut self, index: usize, line: &Line<'a>, from: &'a str) {
        let Some(scalar) = Scalar::read(line, from) else {
            return;
        };

        match scalar.key {
            "repoURL" => self.repository.record(scalar.unquoted()),
            "chart" => self.chart.record(scalar.unquoted()),
            "targetRevision" => self.target.record((index, scalar)),
            _ => {}
        }
    }
}
