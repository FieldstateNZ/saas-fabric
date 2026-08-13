//! What one application of reconciled state did.

/// The outcome of applying a set of resources to a registry.
///
/// # Same revision, different payload — item 50
///
/// A resource can arrive at the exact revision already held but carrying a
/// *different* payload. That should never happen if every publisher bumps
/// the revision on every change, but "should never happen" is exactly the
/// assumption a revision guard exists to police, not lean on. Before
/// [`Self::divergent_payload`] existed, this case fell straight into
/// [`Self::unchanged`]: a reconciler bug — a real content change that forgot
/// to bump the revision — vanished with no error, no log line, nothing to
/// grep for.
///
/// # Why reject rather than accept
///
/// Two ways to handle a same-revision mismatch were on the table: accept the
/// incoming payload (the operator probably meant to change it), or keep what
/// is held and just say so loudly. This picks the second, because accepting
/// it makes the revision meaningless:
///
/// - [`ChangeKind::Updated`](crate::resource::ChangeKind::Updated) events
///   only fire when the revision moves, so silently accepting a
///   same-revision payload would leave *no transition event at all* — the
///   one mechanism this crate has for telling attached state to let go
///   (§19) would simply not fire.
/// - Two publishers racing to write "revision 8" with different payloads
///   would non-deterministically decide the outcome by arrival order, which
///   is precisely the kind of behaviour §20 exists to rule out.
///
/// Rejecting is deterministic, keeps the revision as the single source of
/// truth for "did this resource change" everywhere else in this crate, and
/// leaves a trail: a distinct counter here, and a warn-level log naming the
/// resource kind, key, and revision at the point of application.
///
/// # One key, twice in the same incoming set
///
/// A source is meant to publish a *set*, and nothing in the pipeline makes
/// that true: [`JsonFileSource`](crate::JsonFileSource) deserialises a JSON
/// array straight into a `Vec<T>`, so a reconciler that emits one key twice
/// produces two entries for one resource.
///
/// The first entry that can be *compared against the outgoing snapshot*
/// decides the key; every later one is refused and counted in
/// [`Self::duplicate_rejected`]. That follows the same reasoning as
/// [`Self::divergent_payload`] — silently picking a winner is guessing at what
/// a broken reconciler meant — with one addition: the winner is chosen by
/// *position* rather than by revision, because the revision is exactly the
/// field a duplicated key calls into question. Taking the highest revision
/// would be interpreting data the source has already got wrong.
///
/// Note "can be compared", not "first". An entry that fails validation is
/// refused before any comparison happens, so it does not consume the key and
/// a later valid entry for it is still installed. Reading that rule as plain
/// "first" is what once made `[invalid a@1, valid a@2]` install nothing at
/// all — see `MergedSnapshot::accept`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ApplyReport {
    /// Resources the registry had not seen before.
    pub added: usize,

    /// Resources whose revision advanced.
    pub updated: usize,

    /// Resources that disappeared from the source.
    pub removed: usize,

    /// Resources whose incoming revision was **older** than the one held, and
    /// were therefore ignored.
    pub stale_ignored: usize,

    /// Resources whose incoming copy matched what was already held — same
    /// revision, same payload.
    pub unchanged: usize,

    /// Resources that failed [`RegistryResource::validate`](crate::RegistryResource::validate)
    /// and were therefore never installed.
    ///
    /// Counted separately from every other bucket because it is the only one
    /// that indicates the *source* published something unusable, rather than
    /// something merely old or unchanged. A non-zero value here on a steady
    /// system means a reconciler needs fixing.
    pub invalid_rejected: usize,

    /// Resources at the **same** revision as what is held, but with a
    /// **different** payload. Never applied — see the type-level docs above
    /// for why rejecting is the correct side to fail on. Kept separate from
    /// [`Self::unchanged`] so this case can never hide inside a "nothing
    /// happened" count.
    pub divergent_payload: usize,

    /// Resources refused because an **earlier entry in the same incoming set**
    /// had already decided their key.
    ///
    /// Distinct from every other bucket: the others describe a disagreement
    /// between the source and what is held, while this one describes the
    /// source disagreeing with *itself* inside a single publication. See the
    /// type-level docs for why the first entry wins and the rest are refused.
    pub duplicate_rejected: usize,
}

impl ApplyReport {
    /// Whether anything actually moved.
    ///
    /// Used to decide between an info-level "snapshot applied" line and a
    /// debug-level "nothing changed" one. Most refreshes change nothing, and
    /// logging every one at info would bury the ones that matter.
    ///
    /// Only `added`, `updated`, and `removed` count as movement.
    /// [`Self::divergent_payload`], [`Self::invalid_rejected`] and
    /// [`Self::duplicate_rejected`] deliberately do not: in each case nothing
    /// in the registry moved on account of the refusal, whatever was held is
    /// retained, and every occurrence already gets its own log line at the
    /// point it happens. They are *also* carried on the aggregate line itself,
    /// on both branches, so a debug-level "no changes" is never the only thing
    /// said about a refresh that refused something.
    ///
    /// Every field is bound below rather than matched with `..`, so a bucket
    /// added later cannot default into "not movement" without someone saying
    /// so — the compiler asks the question at the one place able to answer it.
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        let Self {
            added,
            updated,
            removed,
            unchanged: _,
            stale_ignored: _,
            invalid_rejected: _,
            divergent_payload: _,
            duplicate_rejected: _,
        } = *self;

        added == 0 && updated == 0 && removed == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_with_no_movement_is_a_noop() {
        let report = ApplyReport {
            unchanged: 12,
            stale_ignored: 1,
            ..ApplyReport::default()
        };

        assert!(report.is_noop());
    }

    #[test]
    fn any_movement_makes_it_not_a_noop() {
        assert!(!ApplyReport {
            removed: 1,
            ..ApplyReport::default()
        }
        .is_noop());
    }

    #[test]
    fn a_refused_duplicate_is_counted_separately_and_does_not_defeat_noop() {
        // The first entry for the key may well have moved something, and that
        // movement is already counted in its own bucket. The refusal itself
        // moved nothing.
        let report = ApplyReport {
            duplicate_rejected: 1,
            ..ApplyReport::default()
        };

        assert!(report.is_noop());
        assert_eq!(report.duplicate_rejected, 1);
    }

    #[test]
    fn a_divergent_payload_is_counted_separately_and_does_not_defeat_noop() {
        // Nothing in the registry moved (the old payload wins), so the
        // aggregate log line stays at debug — but the count is not zero, and
        // the per-resource warn log fires independently of this.
        let report = ApplyReport {
            divergent_payload: 1,
            ..ApplyReport::default()
        };

        assert!(report.is_noop());
        assert_eq!(report.divergent_payload, 1);
    }
}
