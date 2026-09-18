//! What a client's document asks Fabric to place, as the selector reads it.

use fabric_runtime_publication::PlacementClassDocument;

/// One logical data source's declared intent, as the selector reads it.
///
/// # Why this is not `fabric_client_model::DataIntent`
///
/// They are the same three fields, and that is deliberate: ADR 0023 part 1's
/// argument for `fabric-client-model` reusing [`PlacementClassDocument`]
/// applies here too, one level up the chain -- an intent's `class` and a
/// data source's `placement` must never be able to disagree about what a
/// "shared" or "high availability" class even means.
///
/// What is not shared is the type itself. `fabric-client-model` parses a
/// client's document; this crate holds the rules that decide where a data
/// source lives, and neither may depend on the other --
/// `docs/architecture/crate-dependencies.md` gives only `fabric-control-plane`
/// that edge, because only it composes a client's document with the
/// platform's declared data sources. So this is a structural twin,
/// converted by the one caller that has both types in scope, the same way
/// `fabric-runtime-publication` declares its own copy of a wire shape rather
/// than reach across a boundary it may not cross.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataIntent {
    /// The service class this intent asks for.
    pub class: PlacementClassDocument,

    /// Free text, carried into a refusal message and never matched: nothing
    /// declares a provider on a data source yet.
    pub provider: Option<String>,

    /// Matched against a candidate data source's `residency.region` when
    /// present, exactly. Absent means any region.
    pub region: Option<String>,
}
