//! The platform's machine credential for a Git host.

/// How SaaS Fabric authenticates to the Git host.
///
/// # Two shapes, and only one of them is the production posture
///
/// [`App`](Self::App) is what a deployment uses: a GitHub App installed on the
/// repositories the operator selected and nothing else. The platform holds a private key
/// and mints a short-lived installation token per hour, so the durable secret
/// is never a bearer token and the token that *is* a bearer expires on its own.
///
/// [`Token`](Self::Token) presents a value as-is. It exists because the
/// adapter's tests drive a real socket and need something to present, and
/// because a Git host that is not GitHub would use it. It is **not** the
/// posture to deploy: a token long-lived enough to sit in a secret store is a
/// token that outlives whoever issued it.
///
/// # Redacting, in both shapes
///
/// No [`Display`](std::fmt::Display), and a [`Debug`] that prints a fixed
/// string. A `String` here is one `{:?}` on a config struct away from putting
/// write access to the platform's desired state into a log aggregator, and the
/// code that leaks it looks exactly like the code that does not.
#[derive(Clone)]
pub enum GitCredential {
    /// A bearer value presented unchanged.
    Token(String),

    /// A GitHub App installation, which mints its own short-lived tokens.
    App {
        /// The App's identifier, which becomes the JWT's issuer.
        app_id: String,

        /// The installation this integration uses.
        ///
        /// An App can be installed on several accounts; a token is minted per
        /// *installation*, and this names the one that has access to the one
        /// repository this integration writes to.
        installation_id: String,

        /// The App's RSA private key, in PEM form.
        private_key: String,
    },
}

impl GitCredential {
    /// Wraps a bearer value presented unchanged.
    #[must_use]
    pub fn token(value: impl Into<String>) -> Self {
        Self::Token(value.into())
    }

    /// Wraps a GitHub App installation's identifiers and key.
    #[must_use]
    pub fn app(
        app_id: impl Into<String>,
        installation_id: impl Into<String>,
        private_key: impl Into<String>,
    ) -> Self {
        Self::App {
            app_id: app_id.into(),
            installation_id: installation_id.into(),
            private_key: private_key.into(),
        }
    }
}

impl std::fmt::Debug for GitCredential {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The variant is named because it is a *posture*, and knowing which one
        // a process is running is worth having in a log. Nothing inside either
        // variant is printed — not the key, and not the identifiers, since an
        // App id plus an installation id is most of what an attacker needs to
        // know which key to look for.
        let posture = match self {
            Self::Token(_) => "Token",
            Self::App { .. } => "App",
        };

        write!(formatter, "GitCredential::{posture}(redacted)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_does_not_contain_a_token() {
        let credential = GitCredential::token("ghp_notarealtoken");

        assert!(!format!("{credential:?}").contains("ghp_"));
    }

    #[test]
    fn debug_output_does_not_contain_a_private_key_or_its_identifiers() {
        let credential = GitCredential::app("123456", "789", "-----BEGIN RSA PRIVATE KEY-----");

        let rendered = format!("{credential:?}");

        assert!(!rendered.contains("BEGIN"));
        assert!(!rendered.contains("123456"));
        assert!(!rendered.contains("789"));
    }

    #[test]
    fn debug_output_names_the_posture() {
        // Which posture a process is running is worth seeing in a log; what is
        // inside it is not.
        assert!(format!("{:?}", GitCredential::token("x")).contains("Token"));
        assert!(format!("{:?}", GitCredential::app("1", "2", "k")).contains("App"));
    }

    #[test]
    fn a_struct_deriving_debug_around_it_stays_safe() {
        // The realistic leak: not `{:?}` on the credential, but `{:?}` on a
        // config struct that happens to hold one.
        #[derive(Debug)]
        struct Holder {
            #[allow(dead_code)]
            credential: GitCredential,
        }

        let holder = Holder {
            credential: GitCredential::token("ghp_notarealtoken"),
        };

        assert!(!format!("{holder:?}").contains("ghp_"));
    }
}
