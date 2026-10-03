//! Whether a selection's resolution fits in one request.

/// Refuses a resolution budget that is zero, or that could let a selection
/// outlast the request that asked for it.
///
/// # The sum, and why a Git call on each side
///
/// Selecting a component version reads the catalogue from Git, resolves the
/// version against the registries under this budget, and writes the
/// catalogue back to Git (ADR 0026 section 7). The budget bounds everything
/// between the two Git calls — the registry store's read of which
/// repositories are registered, and every registry read — and cuts it short;
/// neither Git call is cut short by anything but its own timeout. So the
/// longest a selection can take is a Git read, the budget and a Git write,
/// and that has to sit below the request, or the operator is told `504`
/// about a write that may still land.
///
/// # Errors
///
/// A message naming every field the sum depends on. Never a credential.
pub(super) fn validate(
    resolution_budget_seconds: u64,
    git_http_timeout_seconds: u64,
    request_timeout_seconds: u64,
) -> Result<(), String> {
    if resolution_budget_seconds == 0 {
        return Err(
            "registries.resolution_budget_seconds must be at least 1: zero would refuse every \
             selection before a registry was asked"
                .to_owned(),
        );
    }

    // Saturating, so a deployment that wrote something absurd is refused
    // rather than wrapping round into a sum that looks small enough.
    let longest = git_http_timeout_seconds
        .saturating_add(resolution_budget_seconds)
        .saturating_add(git_http_timeout_seconds);

    if longest >= request_timeout_seconds {
        return Err(format!(
            "git_host.http_timeout_seconds ({git_http_timeout_seconds}) plus \
             registries.resolution_budget_seconds ({resolution_budget_seconds}) plus \
             git_host.http_timeout_seconds ({git_http_timeout_seconds}) must be less than \
             request_timeout_seconds ({request_timeout_seconds}): selecting a component version \
             reads the catalogue from Git, resolves the version within the budget, and writes the \
             catalogue back, and all three have to fit in one request"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate;

    #[test]
    fn the_defaults_fit() {
        assert!(validate(8, 10, 30).is_ok());
    }

    #[test]
    fn a_zero_budget_is_refused() {
        let message = validate(0, 10, 30).expect_err("zero refuses every selection");
        assert!(
            message.contains("registries.resolution_budget_seconds"),
            "{message}"
        );
    }

    #[test]
    fn a_budget_that_fits_only_without_both_git_calls_is_refused() {
        // 10 + 10 + 10 is exactly the request: not below it.
        let message = validate(10, 10, 30).expect_err("the sum must be strictly below the request");
        for field in [
            "git_host.http_timeout_seconds (10)",
            "registries.resolution_budget_seconds (10)",
            "request_timeout_seconds (30)",
        ] {
            assert!(message.contains(field), "{message}");
        }
        assert!(
            validate(20, 5, 30).is_err(),
            "a budget that fits alone is not enough"
        );
    }

    #[test]
    fn an_absurd_budget_is_refused_rather_than_wrapping() {
        assert!(validate(u64::MAX, 10, 30).is_err());
    }
}
