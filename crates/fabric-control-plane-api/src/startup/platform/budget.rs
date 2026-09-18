//! Whether a configured operation budget can drain within one request.

use crate::config::PlatformManagementConfig;

/// Refuses a budget that could let an operator's disconnect outlast the
/// request that waits for it.
///
/// See [`establish`](super::establish)'s own rustdoc for the full argument
/// ("The maximum drain has to sit below a request, with room left over");
/// this is only the arithmetic that argument requires.
///
/// # Errors
///
/// A message naming every field the sum depends on. Never a credential.
pub(super) fn validate(
    config: &PlatformManagementConfig,
    http_timeout_seconds: u64,
    request_timeout_seconds: u64,
) -> Result<(), String> {
    if config.operation_timeout_seconds == 0 {
        return Err(
            "platform_management.operation_timeout_seconds must be at least 1: zero would time \
             out every operation immediately"
                .to_owned(),
        );
    }

    // Saturating, so a deployment that wrote something absurd is refused
    // rather than wrapping round into a sum that looks small enough.
    let longest = config
        .operation_timeout_seconds
        .saturating_add(http_timeout_seconds);

    if longest >= request_timeout_seconds {
        return Err(format!(
            "platform_management.operation_timeout_seconds ({}) plus git_host.http_timeout_seconds \
             ({http_timeout_seconds}) must be less than request_timeout_seconds \
             ({request_timeout_seconds}): an operator's disconnect waits for the operation already \
             in flight, which runs for the budget plus the one call the budget cannot cut short, \
             and that wait has to sit below one request with room left for the rest of the \
             disconnect",
            config.operation_timeout_seconds
        ));
    }

    Ok(())
}
