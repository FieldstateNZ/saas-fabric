//! Where in an Argo Application a line sits.

/// Where in an Argo Application the walk currently is.
///
/// Argo puts a chart source in `spec.sources`, and nowhere else in these files
/// is a list of things with a `chart:` and a `targetRevision:`. Tracking the
/// two keys that lead there is what stops this renderer from being willing to
/// edit any list that happens to look similar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Position {
    /// Not under `spec:` at all.
    Outside,

    /// Under `spec:`, at the indent its keys sit at.
    Spec(usize),

    /// Under `spec.sources:`, at the indent its entries sit at.
    SourceList(usize),
}

impl Position {
    /// Follows the walk into and out of `spec.sources`.
    pub(super) fn observe(&mut self, trimmed: &str, indent: usize) {
        match *self {
            Self::Outside => {
                if indent == 0 && trimmed.starts_with("spec:") {
                    *self = Self::Spec(indent);
                }
            }
            Self::Spec(spec) => {
                // A key back at the document's top level has left `spec`.
                if indent <= spec && !trimmed.is_empty() && !trimmed.starts_with('#') {
                    *self = Self::Outside;
                    self.observe(trimmed, indent);
                } else if trimmed.starts_with("sources:") {
                    *self = Self::SourceList(indent);
                }
            }
            Self::SourceList(sources) => {
                // Any key at or above `sources:`' own indent ends the list.
                if indent <= sources
                    && !trimmed.starts_with("- ")
                    && !trimmed.is_empty()
                    && !trimmed.starts_with('#')
                {
                    *self = Self::Outside;
                    self.observe(trimmed, indent);
                }
            }
        }
    }

    /// Whether the walk is inside the sources list.
    pub(super) const fn inside(self) -> bool {
        matches!(self, Self::SourceList(_))
    }

    /// Whether a `- ` at this indent starts a source rather than something
    /// nested inside one.
    pub(super) const fn entry_at(self, indent: usize) -> bool {
        matches!(self, Self::SourceList(sources) if indent > sources)
    }
}
