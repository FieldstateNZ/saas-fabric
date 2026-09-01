//! Text that is safe to put in front of an operator.

mod redaction;

#[cfg(test)]
mod diagnostic_tests;

use redaction::redact;

/// The longest a diagnostic may be.
///
/// A console line, not a log. The cap is the part that survives a mistake
/// nobody anticipated: a response body, a struct dumped with `Debug`, a
/// certificate chain — whatever it is, only the first sentence of it arrives,
/// and a truncated leak is still a leak but a much smaller one.
const MAX: usize = 200;

/// Prefixes that begin a credential, and everything after one is redacted.
///
/// Not a complete list, and cannot be. It covers the shapes this platform
/// actually handles, and the structural rules below cover the shapes it does
/// not know about yet.
pub(super) const CREDENTIAL_PREFIXES: [&str; 8] = [
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    // OpenBao and Vault service and batch tokens.
    "hvs.",
    "hvb.",
];

/// A diagnostic that may be shown to an operator.
///
/// # Why this is a type and not a convention
///
/// The failure it guards against is somebody, later, formatting an upstream
/// error into a status message — a `Debug` on a response, a transport error
/// carrying a signed URL, an auth failure quoting the header it was sent. Each
/// adapter is careful about this today; a convention is only as good as the
/// next person to add one.
///
/// So the only way to obtain one of these is through [`sanitise`](Self::sanitise),
/// and the record that reaches a console cannot hold anything else.
///
/// # What it is not
///
/// A proof. It is a tripwire, and the last line of a defence whose first line
/// is adapters that classify their own failures rather than forwarding
/// somebody else's words. Anything genuinely secret should not have reached a
/// message in the first place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeDiagnostic(String);

impl SafeDiagnostic {
    /// Redacts what looks like a credential, and truncates the rest.
    #[must_use]
    pub fn sanitise(text: &str) -> Self {
        let redacted = redact(text);

        let capped = match redacted.char_indices().nth(MAX) {
            None => redacted,
            Some((cut, _)) => {
                let mut short = redacted.get(..cut).unwrap_or_default().to_owned();
                short.push('…');
                short
            }
        };

        Self(capped)
    }

    /// The text, safe to display.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for SafeDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
