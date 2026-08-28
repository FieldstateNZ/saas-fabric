//! How closely a connector's operator matches the meaning that was asked for.

/// How well a connector's declared operator satisfies a semantic.
///
/// # Why this distinction has to be carried
///
/// [`SemanticOperator`](super::SemanticOperator) has one `Contains`, but NDC
/// has two operators that answer to it: `contains` and `contains_insensitive`.
/// `scalar-types.md` defines them as genuinely different predicates — one
/// tests a substring, the other tests it case-insensitively — so a scalar
/// declaring both has an exact answer to give and a widened one.
///
/// Without this type the index kept whichever name came first in `BTreeMap`
/// order, so `_ilike` beat `_like` on the strength of the leading `i`, and a
/// caller asking for containment silently got the case-insensitive predicate.
/// Alphabetical order is not a semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OperatorFit {
    /// The connector's definition *is* the semantic that was asked for.
    Exact,

    /// A different predicate, accepted in the semantic's place and only while
    /// the scalar declares nothing exact.
    Widened,
}
