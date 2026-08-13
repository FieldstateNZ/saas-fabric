//! The rule that a *first* load must never install an empty set.

use crate::resource::RegistryResource;

/// The first rejection reason when **every** resource in a non-empty set fails
/// validation, or `None` when there is at least one to install.
///
/// # Why an all-invalid first load is a load failure
///
/// [`RegistryResource::validate`] drops a bad resource and keeps the copy
/// already held, and its rationale is explicit about why: one operator's typo
/// must not freeze every other tenant's updates. That reasoning depends
/// entirely on there *being* a previous copy to retain.
///
/// On a first load there is none. A set whose every entry is rejected installs
/// an empty snapshot, and an empty snapshot is not nothing — it makes
/// [`is_primed`](crate::ResourceRegistry::is_primed) true, so `/ready` answers
/// 200 while resolution returns `MissingDataSource`, which the Data API maps to
/// a **500**. Every tenant is down, the probe says the replica is healthy, and
/// the deploy pipeline sees a clean rollout.
///
/// From outside, that is indistinguishable from a source that failed to load —
/// so it gets the same treatment. "A load failure must never become an empty
/// set" is the rule being extended, not a new one.
///
/// # Why one-of-fifty is not the same as fifty-of-fifty
///
/// A partial rejection is left alone, and the asymmetry is the point rather
/// than an omission.
///
/// When something installs, the registry has state to serve and `is_primed`
/// is honest: the tenants whose resources were fine are served, and the ones
/// whose resources were rejected fail closed and individually. Refusing to
/// start would instead take all forty-nine healthy tenants offline over the
/// fiftieth's typo — reintroducing exactly the platform-wide freeze that
/// dropping-with-a-log exists to prevent, and doing it worse, because at prime
/// the blast radius is the whole replica rather than one resource.
///
/// So the question is not "was anything rejected" but "is there anything left
/// to serve". Zero is different in kind from some; one is different from two
/// only in degree. The partial case stays loud without being fatal:
/// [`ApplyReport::invalid_rejected`](crate::ApplyReport::invalid_rejected) is
/// non-zero, each rejection logs at error level, and the primed count reports
/// what was actually installed rather than what the source offered.
///
/// # Why this is checked before applying
///
/// [`apply_all`](crate::ResourceRegistry::apply_all) primes the registry as a
/// side effect and nothing can un-prime it. A set that is about to be declared
/// a load failure therefore has to be refused while `is_primed` is still
/// false — otherwise `fail_fast_on_prime: false` would start a process that is
/// primed and empty, which is the original defect wearing a config flag.
///
/// The cost is one extra `validate` pass over a set already in memory, once per
/// process start. The refresh path pays nothing: it never needs this check,
/// because by then a previous copy exists for every held key and `apply_all`
/// retains it.
pub(super) fn first_rejection_if_nothing_is_usable<T: RegistryResource>(resources: &[T]) -> Option<String> {
    let mut first = None;

    for resource in resources {
        // Short-circuits on the first usable resource: the question is only
        // whether *anything* can be served, not how much.
        let Err(error) = resource.validate() else {
            return None;
        };

        if first.is_none() {
            first = Some(format!("{} {}: {error}", T::KIND, resource.key()));
        }
    }

    first
}
