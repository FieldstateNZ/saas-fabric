//! Refusing a health detail role that authorises more than it names.

/// Requires the health detail role to name an actual role.
///
/// # Why a blank role is refused rather than read as "nobody"
///
/// [`may_see_detail`](crate::health) already reads blank as "nobody is
/// authorised". The refusal is still worth having: a blank value is almost
/// always a template that did not render, and before ADR 0027 the same
/// setting also fed the Data API's administrator bypass, where a blank role
/// granted every operation to any token carrying an empty role. That bypass
/// is gone, and refusing the value keeps the probe from becoming the next
/// consumer to read blank the open way.
///
/// A deployment that genuinely wants nobody to see the detail names a role
/// nothing holds, which says so explicitly rather than by omission.
///
/// # Errors
///
/// Returns a message if the role is empty or whitespace.
pub(super) fn validate(role: &str) -> Result<(), String> {
    if role.trim().is_empty() {
        return Err(
            "health_detail_role must name a role: a blank value is a template that did not \
                    render. To let nobody see probe detail, name a role nothing holds"
                .to_owned(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_role_is_rejected() {
        assert!(validate("").is_err());
    }

    #[test]
    fn a_whitespace_role_is_rejected_with_it() {
        assert!(validate("   ").is_err());
    }

    #[test]
    fn the_rejection_says_how_to_authorise_nobody() {
        assert!(validate("").is_err_and(|message| message.contains("name a role nothing holds")));
    }

    #[test]
    fn a_named_role_is_accepted() {
        assert!(validate("platform-admin").is_ok());
    }
}
