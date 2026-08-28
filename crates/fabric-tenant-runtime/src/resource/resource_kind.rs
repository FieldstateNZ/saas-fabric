//! What a type must supply to live in a registry.

use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::sync::Arc;

use fabric_core::BindingRevision;

use crate::ConfigurationError;

/// A reconciled runtime resource.
///
/// Five things are required, and they are what the lifecycle turns on: an
/// identity to look the resource up by, a revision to decide whether an
/// incoming copy is newer than the one held (§20), equality, a coherence
/// check run before the resource may be served, and the whole-set facts that
/// no single resource can answer for itself.
///
/// The `PartialEq` bound exists for one reason: the apply path needs to tell
/// a genuine no-op (same revision, same payload) apart from a same-revision
/// payload that quietly diverged — a reconciler bug that forgot to bump the
/// revision. See [`ApplyReport::divergent_payload`](crate::resource::ApplyReport::divergent_payload)
/// for why that distinction is load-bearing rather than cosmetic.
///
/// `KIND` exists so that log lines and errors can say *what* was not found
/// without the generic code knowing anything else about the type.
pub trait RegistryResource: Clone + PartialEq + Send + Sync + 'static {
    /// How this resource is addressed.
    type Key: Clone + Ord + Hash + Display + Send + Sync + 'static;

    /// A fact about a whole snapshot that no single resource can answer.
    ///
    /// [`Self::validate`] only ever sees one resource, so it can only ever ask
    /// questions about one. Two of the rules this runtime has to enforce are
    /// questions about the *set* — "is more than one tenant bound to this
    /// DataSource?", "do two DataSources name the same physical destination?"
    /// — and the answers decide whether a tenant boundary is real. No
    /// per-resource check can reach them.
    ///
    /// Deriving them here, as the snapshot is built, is what keeps the request
    /// path O(1). The scan runs on the refresh path, which is already paying
    /// for a full rebuild of the map, and resolution then reads the answer off
    /// the snapshot it was loading anyway rather than walking every tenant per
    /// request.
    ///
    /// A resource kind with no such fact answers `()`.
    type SetFacts: Debug + Send + Sync + 'static;

    /// A human-readable name for this kind of resource, such as `tenant`.
    const KIND: &'static str;

    /// This resource's identity.
    fn key(&self) -> &Self::Key;

    /// This resource's revision. Only ever moves forward.
    fn revision(&self) -> BindingRevision;

    /// Checks the resource is coherent enough to serve.
    ///
    /// Called by every registry mutator before a resource may enter a
    /// snapshot, so a tenant with no data bindings or a pool that can never
    /// hand out a connection is caught when it loads rather than as a scatter
    /// of failures on the request path hours later.
    ///
    /// There is deliberately **no default implementation**. A blanket
    /// `Ok(())` would let a resource type added later opt out of validation by
    /// saying nothing at all, which is exactly how this check came to be
    /// skipped in the first place. Requiring it makes the compiler ask the
    /// question, and a type with genuinely nothing to check answers it in one
    /// line.
    ///
    /// # Why an invalid resource is dropped rather than failing the apply
    ///
    /// The alternative — reject the whole incoming set if any one resource is
    /// bad — was rejected on two counts:
    ///
    /// - It converts one operator's typo into a platform-wide freeze. Every
    ///   other tenant's legitimate revision bump would queue behind the bad
    ///   resource until a human fixed it, and because a refused apply leaves
    ///   the previous snapshot serving, nothing would appear to be wrong.
    /// - From the registry's side it is indistinguishable from a source that
    ///   failed to load, and this crate already holds the line that a load
    ///   failure must never become an empty set
    ///   ([`apply_all`](crate::ResourceRegistry::apply_all)). Skipping only
    ///   the offender keeps that rule and lets everything else make progress.
    ///
    /// # Why the held copy is retained rather than removed
    ///
    /// If a resource that is already being served arrives in an unusable
    /// state, the copy already held stays in the snapshot. Absence from the
    /// incoming set is how this crate expresses deprovisioning; an unusable
    /// *payload* is a reconciler bug, and treating a bug as an instruction to
    /// take a live tenant offline is the wrong side to fail on. It also
    /// matches how the same-revision divergent-payload case is handled: keep
    /// what is held, and say so loudly.
    ///
    /// Either way the rejection is counted in
    /// [`ApplyReport::invalid_rejected`](crate::ApplyReport::invalid_rejected)
    /// and logged at error level, per resource, on every attempt — so a
    /// problem that persists keeps being visible rather than scrolling past
    /// once.
    ///
    /// # Errors
    ///
    /// [`ConfigurationError`] describing what is wrong, in terms specific to
    /// the resource kind.
    fn validate(&self) -> Result<(), ConfigurationError>;

    /// Derives [`Self::SetFacts`] for the set that is about to be installed.
    ///
    /// Called from `ResourceSnapshot::new`, which is the only way a map
    /// becomes a snapshot. That placement is the point: the facts are derived
    /// from the very map they describe, so there is no call site at which a
    /// snapshot could be installed carrying facts about some other set — the
    /// same argument `MergedSnapshot` makes for letting the merge, and only
    /// the merge, decide whether a set is usable.
    ///
    /// Note what this must **not** do: refuse anything. It reports; the
    /// request path decides. Dropping a resource here would express a
    /// misconfiguration as `UnknownTenant` — a terminal 403 telling a caller
    /// their tenant does not exist — when the honest answer is that the
    /// platform cannot currently serve it.
    fn derive_set_facts(entries: &HashMap<Self::Key, Arc<Self>>) -> Self::SetFacts;
}
