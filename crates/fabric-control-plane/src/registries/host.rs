//! The name a registry is known by: the host its repositories are named
//! under.

use fabric_component::Repository;

/// A registry's host, with its port if it has one: `ghcr.io`, `docker.io`,
/// `registry.example.com:5000`.
///
/// # Why the rule is `Repository`'s, applied through it
///
/// A registry is named by its host because every image reference names it
/// that way (ADR 0026 section 5), and a host this accepted that no
/// repository could be written under would be a registry nothing could ever
/// be read from. So there is one rule, not two that could drift: the host
/// is checked as the first component of a repository on it — lower case,
/// DNS labels, a name and never an IP address, a port that is not 443, and
/// never one of Docker Hub's other spellings.
///
/// # A lookup key, never a location
///
/// It selects a record this platform already holds. Nothing builds a URL,
/// a store path or a secret name from it: the endpoint is the record's, and
/// the secret's name is an id the server minted.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RegistryHost(String);

impl RegistryHost {
    /// Parses a host.
    ///
    /// # Errors
    ///
    /// A message naming the rule the host broke.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.is_empty() || text.contains(['/', '@', '?', '#']) || text.chars().any(char::is_whitespace) {
            return Err(
                "a registry is named by its host alone, with its port if it has one, such as ghcr.io"
                    .to_owned(),
            );
        }
        // Two path segments, so Docker Hub's one-segment rule is not what
        // answers for a host.
        Repository::try_new(format!("{text}/library/probe"))
            .map(|_| Self(text.to_owned()))
            .map_err(|error| error.to_string())
    }

    /// The host, as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RegistryHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl TryFrom<String> for RegistryHost {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<RegistryHost> for String {
    fn from(value: RegistryHost) -> Self {
        value.0
    }
}
