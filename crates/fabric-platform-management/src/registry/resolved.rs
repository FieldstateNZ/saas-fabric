//! What a registry knows about one reference: a digest, and where it came from.

/// What an artifact says about where it came from.
///
/// # Where a revision is read from
///
/// An image's revisions are the `org.opencontainers.image.revision` values in
/// its config's labels and in its manifest's annotations (ADR 0026 section
/// 3): a build may stamp either, and one that stamps both must stamp the
/// same commit. Exactly one distinct value is [`Agreed`](Self::Agreed), none
/// is [`Absent`](Self::Absent), more than one is
/// [`Disagreed`](Self::Disagreed).
///
/// For an index, each **deployable** child — one declaring a concrete
/// platform, which leaves out the `unknown/unknown` attestations build
/// systems put beside an image — has the set of its own labels, its own
/// annotations and the index's annotations. Every child must hold exactly
/// one value and all must agree. A child holding none makes the index
/// `Absent`, unless some child disagrees, because disagreement is final and
/// waiting is not going to resolve it. An index with no deployable child is
/// `Absent`: zero children cannot agree with each other.
///
/// # Why absence and disagreement are not the same answer
///
/// An artifact carrying no revision may simply still be publishing — a push
/// in flight looks identical to a label that was never set, and waiting is the
/// cheaper mistake. An artifact whose parts *disagree* about their source
/// commit is one version built twice, and no amount of waiting resolves it.
///
/// Collapsing them would either retry a broken build forever or refuse a
/// perfectly ordinary publishing window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Everything inspected agrees it came from this commit.
    Agreed(String),

    /// Something inspected carries no revision at all.
    Absent,

    /// The parts inspected name different commits.
    Disagreed,
}

/// What a registry knows about one reference in one repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The manifest digest, which is what a deployment pins: `sha256:` and
    /// the hex of the SHA-256 of the manifest bytes, computed by the adapter
    /// from the bytes it read — never copied from a response header.
    ///
    /// For a multi-architecture image this is the **index**, so a node still
    /// selects its own architecture. Pinning one platform's manifest would
    /// hand every node the same one.
    pub digest: String,

    /// Where it says it came from.
    ///
    /// An adapter reporting this for an index must satisfy itself that *every*
    /// manifest it inspected agrees. Reading one platform's label proves that
    /// platform's provenance and not the artifact's, and "the architecture we
    /// happen to run today" is not a fact about the image.
    pub provenance: Provenance,
}
