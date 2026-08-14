//! A claim allowlist that cannot be empty.

/// A non-empty list of accepted values for one token claim.
///
/// # Why a newtype rather than a `Vec<String>`
///
/// The three states an operator can express are *not* the three states a
/// `Vec` has. "Do not examine this claim" and "examine it against this list"
/// are the only two the runtime supports; an empty list is neither, and the
/// composition root used to read it as the first — so `issuers = []` skipped
/// the allowlist entirely and accepted every issuer. A placeholder written by
/// an operator, or a template that rendered to nothing, silently turned the
/// check off.
///
/// Making the empty case unrepresentable moves that from a branch somebody has
/// to remember to write into a startup failure nobody can bypass. `Option<Allowlist>`
/// then carries exactly the two supported states: `None` is "not examined",
/// `Some` is always a list with something in it.
///
/// Blank entries are refused for the same reason: an issuer or audience of `""`
/// matches nothing a real identity provider emits, so it is a rendering
/// artefact rather than an intention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Allowlist(Vec<String>);

impl Allowlist {
    /// Builds an allowlist, refusing the states that would weaken a check.
    ///
    /// # Errors
    ///
    /// Returns a message if the list is empty or any entry is blank. The
    /// message names the alternative, because "omit the setting" is not
    /// guessable from "empty is invalid".
    pub fn try_new(values: Vec<String>) -> Result<Self, String> {
        if values.is_empty() {
            return Err(
                "an empty list is not a way to disable this check: it would leave the claim \
                        unexamined. Omit the setting entirely to skip the check, or list at least \
                        one accepted value"
                    .to_owned(),
            );
        }

        if values.iter().any(|value| value.trim().is_empty()) {
            return Err(
                "a blank entry matches nothing an identity provider issues, so it is almost \
                        certainly an unrendered template. Remove it or give it a value"
                    .to_owned(),
            );
        }

        Ok(Self(values))
    }

    /// The accepted values, for handing to the identity crate's builders.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl<'de> serde::Deserialize<'de> for Allowlist {
    /// Deserialises as a list of strings, then applies [`Allowlist::try_new`].
    ///
    /// Rejecting here rather than in a later validation pass is what makes the
    /// error name the setting and the file or environment variable it came
    /// from — figment supplies that context only while it is still parsing.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let values = Vec::<String>::deserialize(deserializer)?;

        Self::try_new(values).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_list_is_a_configuration_error_rather_than_no_allowlist() {
        let error = Allowlist::try_new(Vec::new());

        assert!(error.is_err_and(|message| message.contains("Omit the setting")));
    }

    #[test]
    fn a_blank_entry_is_refused_as_an_unrendered_template() {
        let error = Allowlist::try_new(vec!["https://id.example.com".to_owned(), "  ".to_owned()]);

        assert!(error.is_err_and(|message| message.contains("blank entry")));
    }

    #[test]
    fn a_populated_list_is_carried_through_unchanged() {
        let list = Allowlist::try_new(vec!["https://id.example.com".to_owned()]);

        assert!(list.is_ok_and(|list| list.as_slice() == ["https://id.example.com"]));
    }

    #[test]
    fn deserialising_an_empty_list_fails_rather_than_yielding_an_empty_allowlist() {
        let result = serde_json::from_str::<Allowlist>("[]");

        assert!(result.is_err());
    }
}
