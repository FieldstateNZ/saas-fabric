//! Refusing an administrator role that authorises more than it names.

use fabric_data_api::ResourcePermissions;

/// Requires the administrator role to name an actual role.
///
/// # Why a blank role is refused rather than read as "no administrator"
///
/// The two consumers of this setting disagree about what blank means, and one
/// of them fails *open*.
///
/// | Consumer | `administrator_role = ""` means |
/// |---|---|
/// | [`may_see_detail`](crate::health) — this crate | nobody is authorised; returns early, documented as the fail-closed reading |
/// | `ResourcePermissions::permits` — `fabric-data-api` | `identity.has_role("")`, so any token whose `roles` contains an empty string is granted **every** operation on **every** resource |
///
/// The second is reachable from here: [`serving`](crate::startup) hands
/// `permissions` to `build_data_api` verbatim, so a blank value configured in
/// this file is a live privilege escalation rather than a deployment that
/// happens to have no administrator.
///
/// Refusing the value is the fix available to this crate — `permits` belongs
/// to another one. It makes the disagreement unreachable instead of leaving
/// the fail-open reading live, and a deployment that genuinely wants no
/// administrator names a role nothing holds, which says so explicitly rather
/// than by omission. Whitespace is refused with it: a role of `" "` is a
/// template that did not render.
///
/// # Errors
///
/// Returns a message if the role is empty or whitespace.
pub(super) fn validate(permissions: &ResourcePermissions) -> Result<(), String> {
    if permissions.administrator_role.trim().is_empty() {
        return Err(
            "permissions.administrator_role must name a role: a blank value grants every \
                    operation to any token carrying an empty role. To authorise no administrator, \
                    name a role nothing holds"
                .to_owned(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permissions(administrator_role: &str) -> ResourcePermissions {
        ResourcePermissions {
            administrator_role: administrator_role.to_owned(),
            ..ResourcePermissions::default()
        }
    }

    #[test]
    fn a_blank_role_is_rejected() {
        assert!(validate(&permissions("")).is_err());
    }

    #[test]
    fn a_whitespace_role_is_rejected_with_it() {
        assert!(validate(&permissions("   ")).is_err());
    }

    #[test]
    fn the_rejection_says_how_to_authorise_nobody() {
        assert!(
            validate(&permissions("")).is_err_and(|message| message.contains("name a role nothing holds"))
        );
    }

    #[test]
    fn a_named_role_is_accepted() {
        assert!(validate(&permissions("platform-admin")).is_ok());
    }
}
