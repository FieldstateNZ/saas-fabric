//! How the platform proves who it is to the Git host.

/// How the platform authenticates to the Git host.
///
/// # A tagged enum, so the posture is stated
///
/// The two are not interchangeable. A GitHub App holds a private key and mints
/// an hour-long token per hour; a static token *is* the durable secret and
/// outlives whoever issued it. A deployment should have to write down which one
/// it is running, and a flat struct with optional fields would let it end up in
/// one by omission.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum GitAuthConfig {
    /// A GitHub App installed on the desired-state repository. The production
    /// posture.
    GithubApp {
        /// The App's identifier.
        app_id: String,

        /// The installation that has access to the repository.
        ///
        /// Stated rather than discovered. Looking it up would mean granting
        /// the App enough scope to enumerate its own installations, and
        /// re-doing that lookup on every process start for a value that
        /// changes when somebody reinstalls the App and never otherwise.
        installation_id: String,

        /// **The name of the private key**, not the key.
        private_key_ref: String,
    },

    /// A bearer value presented as-is.
    ///
    /// For a Git host that is not GitHub, and for local development. Not the
    /// posture to deploy — see [`GitCredential`](crate::GitCredential).
    Token {
        /// **The name of the token**, not the token.
        token_ref: String,
    },
}

impl Default for GitAuthConfig {
    /// The production posture, with the names the platform projects.
    ///
    /// A default at all because the surrounding config has one; the values are
    /// references to secrets that must exist, so a deployment that inherits
    /// this and supplies nothing fails at startup rather than running
    /// unauthenticated.
    fn default() -> Self {
        Self::GithubApp {
            app_id: String::new(),
            installation_id: String::new(),
            private_key_ref: "git/saas-fabric-clients-app-key".to_owned(),
        }
    }
}

impl GitAuthConfig {
    /// The secret reference this posture needs resolved.
    #[must_use]
    pub fn secret_ref(&self) -> &str {
        match self {
            Self::GithubApp { private_key_ref, .. } => private_key_ref,
            Self::Token { token_ref } => token_ref,
        }
    }

    /// Checks the settings that would otherwise fail on the first request.
    ///
    /// # Errors
    ///
    /// Returns a message if a required name is empty. An App with no id would
    /// mint a JWT no issuer recognises, and the failure would arrive as a
    /// `401` an operator reads as a bad key.
    pub fn validate(&self) -> Result<(), String> {
        let named: Vec<(&str, &String)> = match self {
            Self::GithubApp {
                app_id,
                installation_id,
                private_key_ref,
            } => vec![
                ("auth.app_id", app_id),
                ("auth.installation_id", installation_id),
                ("auth.private_key_ref", private_key_ref),
            ],
            Self::Token { token_ref } => vec![("auth.token_ref", token_ref)],
        };

        for (name, value) in named {
            if value.trim().is_empty() {
                return Err(format!("clients repository: {name} must not be empty"));
            }
        }

        Ok(())
    }
}
