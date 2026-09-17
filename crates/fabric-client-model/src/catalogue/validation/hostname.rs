//! An application's hostname template is checked twice, against two
//! different rules, because the two checks answer different questions.

use super::invalid;
use crate::catalogue::ApplicationDefinition;
use crate::{DesiredStateError, Host, RedirectStrategy, RedirectStrategyKind, RedirectUri};

/// The placeholder an application's `domain` is written with.
const PLACEHOLDER: &str = "{client}";

/// The longest a whole DNS label may be — what a real client id substituted
/// into `{client}`'s own label must still fit inside, alongside whatever
/// else that label carries.
const LONGEST_LABEL: usize = 63;

impl ApplicationDefinition {
    /// Checks the hostname template, more strictly while publishing.
    ///
    /// # Why saving and publishing ask different questions
    ///
    /// A draft is an operator's work in progress, so saving only refuses a
    /// template that could never be a hostname — more than one `{client}`,
    /// or a shape [`Host`] would refuse outright. Publishing is the point
    /// this definition becomes the one every future assignment resolves
    /// against, so it asks the stricter question an assignment will actually
    /// need answered: exactly one `{client}`, and a real callback built from
    /// it that the platform's own `claimedHttps` strategy would accept.
    /// Deferring that second question to assignment time is how a domain of
    /// `{client}.internal` or `{client}.example.test` used to publish
    /// successfully and then refuse every client it was assigned to.
    ///
    /// # Why the worst case is not always a 63-character client id
    ///
    /// A template like `{client}-portal.example.com` shares its first DNS
    /// label with seven literal characters besides the placeholder, so the
    /// longest client id that label could ever hold is `63 - 7`, not the
    /// full 63 a bare `{client}.example.com` allows — checking against 63
    /// regardless would refuse the affixed form for every client, including
    /// ones whose id could fit it fine. The worst case substituted here is
    /// sized to the label `{client}` actually shares, floored at one
    /// character so a template cannot shrink the check to nothing. A real
    /// client id longer than what its own label has room for is not this
    /// check's problem: `ClientDocument::with_application_identity` already
    /// refuses that specific combination the moment such a client is
    /// assigned this application, which is where a fix belongs — narrowing
    /// one client's id or the template's own affix — that this
    /// catalogue-wide check has no business making for every other client.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError::InvalidField`] for a template with more
    /// than one `{client}`, one [`Host`] would refuse, or — while
    /// publishing — one with no `{client}` at all, or whose worst-case
    /// callback the `claimedHttps` strategy does not admit.
    pub(super) fn validate_domain(&self, publishing: bool) -> Result<(), DesiredStateError> {
        if self.domain.is_empty() {
            return Ok(());
        }

        let occurrences = self.domain.matches(PLACEHOLDER).count();
        if occurrences > 1 {
            return Err(invalid(format!(
                "Application hostname template may contain at most one {PLACEHOLDER}"
            )));
        }

        if Host::try_new(self.domain.replace(PLACEHOLDER, "example")).is_err() {
            return Err(invalid("Invalid application hostname template"));
        }

        if !publishing {
            return Ok(());
        }

        if occurrences != 1 {
            return Err(invalid(format!(
                "Publishing requires the hostname template to contain exactly one {PLACEHOLDER}"
            )));
        }

        // The label `{client}` shares with any literal affix, minus the
        // placeholder itself — what a substituted client id has to leave
        // room for. `occurrences == 1` was just checked above, so the label
        // this finds necessarily contains the placeholder and is at least
        // as long as it.
        let affix_len = self
            .domain
            .split('.')
            .find(|label| label.contains(PLACEHOLDER))
            .map_or(0, |label| label.len() - PLACEHOLDER.len());
        let worst_case_client_len = LONGEST_LABEL.saturating_sub(affix_len).max(1);

        let host = self
            .domain
            .replace(PLACEHOLDER, &"a".repeat(worst_case_client_len));
        let callback = RedirectUri::try_new(format!("https://{host}/callback"))
            .map_err(|error| invalid(format!("Hostname template produces an invalid callback: {error}")))?;

        RedirectStrategy::try_new(RedirectStrategyKind::ClaimedHttps, vec![callback])
            .map_err(|error| invalid(format!("Hostname template's callback is not admitted: {error}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(domain: &str) -> ApplicationDefinition {
        ApplicationDefinition {
            domain: domain.into(),
            ..ApplicationDefinition::default()
        }
    }

    #[test]
    fn no_placeholder_is_fine_at_either_stage() {
        definition("").validate_domain(false).unwrap();
        definition("").validate_domain(true).unwrap();
    }

    #[test]
    fn two_placeholders_are_refused_even_while_only_saving() {
        assert!(definition("{client}.{client}.example.com")
            .validate_domain(false)
            .is_err());
    }

    #[test]
    fn a_single_placeholder_saves_even_on_a_domain_publishing_will_refuse() {
        // The interesting case: `.internal` is a legitimate draft in
        // progress and must not be refused before an operator is ready to
        // publish it.
        definition("{client}.internal").validate_domain(false).unwrap();
    }

    #[test]
    fn publishing_requires_the_placeholder_to_appear() {
        let error = definition("static.example.com")
            .validate_domain(true)
            .unwrap_err();

        assert!(matches!(error, DesiredStateError::InvalidField { .. }));
    }

    #[test]
    fn publishing_refuses_a_private_network_domain() {
        let error = definition("{client}.internal").validate_domain(true).unwrap_err();

        assert!(matches!(error, DesiredStateError::InvalidField { .. }), "{error}");
    }

    #[test]
    fn publishing_refuses_a_reserved_testing_domain() {
        let error = definition("{client}.example.test")
            .validate_domain(true)
            .unwrap_err();

        assert!(matches!(error, DesiredStateError::InvalidField { .. }), "{error}");
    }

    #[test]
    fn publishing_accepts_an_ordinary_public_domain() {
        definition("{client}.example.com").validate_domain(true).unwrap();
    }

    #[test]
    fn publishing_accepts_a_template_with_an_affix_sharing_the_placeholders_label() {
        // A bare `{client}` allows a 63-character worst case; sharing its
        // label with `-portal` (7 characters) must not check against that
        // same 63 — it would make this template unpublishable for every
        // client, not only ones whose id is actually too long.
        definition("{client}-portal.example.com")
            .validate_domain(true)
            .unwrap();
        definition("app-{client}.example.com")
            .validate_domain(true)
            .unwrap();
    }
}
