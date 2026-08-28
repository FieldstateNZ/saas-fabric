//! What a merge decided, held back until someone chooses to install it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::resource::snapshot::ResourceSnapshot;
use crate::resource::{ApplyReport, RegistryResource, ResourceChange};
use crate::UnusableFirstLoad;

/// The snapshot an incoming set *would* install, plus the per-key verdicts that
/// built it.
///
/// # Why the merge answers "is this set usable?"
///
/// Priming used to work the other way round. A standalone predicate walked the
/// incoming set asking *"does any entry validate?"*, and only if it said yes did
/// `apply_all` run and decide what to actually install. Two functions, two
/// answers to what is really one question — and they disagreed.
///
/// The predicate short-circuited on the first entry that validated. `apply_all`
/// let the *first* entry for a key consume that key, valid or not. So
/// `[invalid a@1, valid a@2]` looked usable to the predicate while `apply_all`
/// dropped the first entry as invalid and the second as a duplicate: the
/// registry primed with **nothing**, `is_primed` flipped true and cannot be
/// un-flipped, `/ready` answered 200 over zero tenants, and `fail_fast_on_prime`
/// had nothing to fire on because the load returned `Ok`.
///
/// The drift is the symptom. The defect is that a predicate re-derived another
/// function's decision at all — it would drift again the next time `apply_all`
/// grew a rule. So there is no predicate now. The merge runs first and produces
/// this value; installing it is a separate and *conditional* step. "Did
/// anything survive?" is then read off [`Self::next`] — the very map that will
/// be installed — instead of being predicted from the input.
///
/// The invariant that buys, and the one to preserve: **nothing decides whether
/// a set is usable except the code that decides what to install.**
pub(super) struct MergedSnapshot<T: RegistryResource> {
    /// The map that would replace the current snapshot.
    pub(super) next: HashMap<T::Key, Arc<T>>,

    /// Transitions to publish — but only once `next` is really installed, so a
    /// refused merge announces nothing.
    pub(super) events: Vec<ResourceChange<T::Key>>,

    /// What each incoming entry was counted as.
    pub(super) report: ApplyReport,

    /// Keys already compared against the outgoing snapshot.
    ///
    /// Tracked separately from `next` because the two answer different
    /// questions. `next` may hold a copy that an *invalid* entry put back, and
    /// retaining what is held decides nothing — a later valid entry for that
    /// key must still get its one comparison.
    pub(super) decided: HashSet<T::Key>,

    /// The first validation failure, named, so a refusal can say what to go and
    /// fix rather than only that something is wrong.
    pub(super) first_rejection: Option<String>,

    /// How many resources the incoming set offered. Recorded by [`Self::merge`]
    /// from the very input it judged, so [`Self::refusal`] needs no arguments.
    published: usize,

    /// Whether this merge is against *no* snapshot at all — the registry's first
    /// load, and the only case where installing can be refused.
    first_load: bool,
}

impl<T: RegistryResource> MergedSnapshot<T> {
    /// Merges `incoming` against `current`, touching no registry state.
    ///
    /// Every entry is judged against the snapshot being *replaced*, never
    /// against a partly-built `next` — see [`Self::accept`].
    pub(super) fn merge(current: Option<&ResourceSnapshot<T>>, incoming: Vec<T>) -> Self {
        let mut merged = Self {
            next: HashMap::with_capacity(incoming.len()),
            events: Vec::new(),
            report: ApplyReport::default(),
            decided: HashSet::with_capacity(incoming.len()),
            first_rejection: None,
            published: incoming.len(),
            first_load: current.is_none(),
        };

        for resource in incoming {
            let held = current.and_then(|snapshot| snapshot.get(resource.key()));
            merged.accept(resource, held);
        }

        if let Some(snapshot) = current {
            merged.collect_removals(snapshot);
        }

        merged
    }

    /// Why this merge must not be installed, or `None` when there is something
    /// to serve.
    ///
    /// # Why this takes no arguments
    ///
    /// Both facts it needs — how many resources the source offered, and whether
    /// there was a snapshot to replace — are recorded by [`Self::merge`] from
    /// the very inputs it judged. No caller supplies them, so no caller can
    /// supply a different answer than the merge used, and there is no call site
    /// left that could decide to skip the question.
    ///
    /// # The three conditions
    ///
    /// **A snapshot already exists.** Nothing is refused. A full sync is allowed
    /// to empty a registry — that is how deprovisioning works — and a rejected
    /// resource falls back to the copy held, so something keeps serving either
    /// way. Priming is the irreversible step and it has already happened.
    ///
    /// **Nothing was published.** Zero is legitimate: a deployment that has not
    /// onboarded a tenant yet must still start, and installing nothing is only a
    /// failure when something was offered to install.
    ///
    /// **Something was published and none of it survived.** Indistinguishable
    /// from a source that failed to load, so it gets the same treatment: the
    /// registry is left untouched, which on a first load means unprimed rather
    /// than primed and empty.
    ///
    /// One-of-fifty is deliberately *not* fifty-of-fifty: with something left in
    /// `next`, `is_primed` is honest, the healthy tenants are served, and only
    /// the rejected ones fail closed. Refusing to start there would take
    /// forty-nine tenants down over the fiftieth's typo.
    pub(super) fn refusal(&mut self) -> Option<UnusableFirstLoad> {
        if !self.first_load || self.published == 0 || !self.next.is_empty() {
            return None;
        }

        // The fallback is unreachable today — an empty `next` from a non-empty
        // publication means every entry failed validation, which records a
        // reason. It is spelled out rather than unwrapped both because `unwrap`
        // is denied and because a future rule that drops an entry some other
        // way should still refuse the load, with a message that is merely vague
        // instead of one that is wrong.
        Some(UnusableFirstLoad {
            published: self.published,
            reason: self
                .first_rejection
                .take()
                .unwrap_or_else(|| format!("no {} survived the merge", T::KIND)),
        })
    }
}
