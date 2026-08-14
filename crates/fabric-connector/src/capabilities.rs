//! What a given backend can actually do.

use std::collections::BTreeSet;

use crate::ComparisonOperator;

mod support_check;

/// The features a connector declares support for.
///
/// # Why this is checked rather than assumed
///
/// Backends differ. A document store may not express the same predicates as
/// PostgreSQL; a read-only analytics connector may support no mutations at all.
/// There are two ways to handle that, and only one is safe in a multi-tenant
/// system.
///
/// The unsafe way is to degrade quietly — drop a predicate the backend cannot
/// express and return what you get. In a single-tenant application that is
/// merely wrong. Here, the predicate that gets dropped might be the one
/// restricting rows to the caller's tenant, and the failure looks exactly like
/// success: rows come back, the status code is 200, and nothing is logged.
///
/// So the platform refuses instead. §28 requires failing closed, and an
/// unsupported operation is a case where the safe answer cannot be computed.
///
/// The gate itself is [`ensure_supports_query`](Self::ensure_supports_query)
/// and [`ensure_supports_mutation`](Self::ensure_supports_mutation).
// A flag per capability is the honest representation here: these are
// independent yes/no facts a backend declares, not a state machine that could
// be an enum. Grouping them into sub-structs to satisfy the lint would add
// nesting at every call site and hide nothing.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectorCapabilities {
    /// Whether predicates may be pushed down at all.
    pub filtering: bool,
    /// Whether ordering may be pushed down.
    pub ordering: bool,
    /// Whether `limit`/`offset` may be pushed down.
    pub paging: bool,
    /// Whether the backend accepts writes.
    pub mutations: bool,
    /// Whether several mutations in **one request** are atomic *with respect to
    /// each other*.
    ///
    /// # What this does not mean
    ///
    /// It does not mean an N-row insert is applied all-or-nothing, and reading
    /// it that way is the mistake it invites. The flag is negotiated from NDC's
    /// `mutation.transactional` capability, which in the specification (v0.2.13)
    /// governs the *cardinality of the `operations` array*: without it a caller
    /// must send exactly one operation; with it a caller may send several and
    /// expect them to succeed or fail together. A capability whose entire effect
    /// is "you may now put more than one element in this array" says nothing
    /// about what happens inside one element.
    ///
    /// This platform puts every row of a batch into a single argument of a
    /// single operation, and NDC argument values are opaque JSON — an argument
    /// carrying one row and one carrying five hundred are indistinguishable to
    /// the protocol. Atomicity of a batch is therefore the procedure's private
    /// business, which no capability exposes.
    ///
    /// # Why it is declared but not consulted
    ///
    /// Nothing reads it, on purpose. It is negotiated because a future caller
    /// that genuinely sends multiple operations would need it, and it is
    /// documented here so that the next person to reach for it as a guard on
    /// batch writes finds out why it cannot serve that role. The guard that
    /// does exist is `fabric-data-api`'s `execution::write_integrity`, which
    /// compares the reported affected-row count against the rows actually sent.
    pub transactional_mutations: bool,
    /// Whether the backend can report a total row count ignoring paging.
    pub total_count: bool,
    /// Whether the backend can test a field for null
    /// ([`Filter::IsNull`](crate::Filter::IsNull)).
    ///
    /// A flag of its own rather than an entry in [`comparisons`](Self::comparisons)
    /// because a null test is unary — there is no literal to compare against —
    /// and no comparison stands in for it. Under three-valued logic `x = NULL`
    /// is unknown for every row, so a backend declaring equality has said
    /// nothing about whether it can find nulls.
    pub null_checks: bool,
    /// The comparison operators the backend can express.
    pub comparisons: BTreeSet<ComparisonOperator>,
}

impl ConnectorCapabilities {
    /// The minimum a connector must support to be useful: reads with
    /// predicates, ordering, paging, and equality.
    ///
    /// Every capability beyond that minimum starts `false`. A backend gets a
    /// feature by declaring it, never by a default nobody revisited.
    #[must_use]
    pub fn baseline() -> Self {
        Self {
            filtering: true,
            ordering: true,
            paging: true,
            mutations: false,
            transactional_mutations: false,
            total_count: false,
            null_checks: false,
            comparisons: [ComparisonOperator::Equal, ComparisonOperator::NotEqual]
                .into_iter()
                .collect(),
        }
    }
}
