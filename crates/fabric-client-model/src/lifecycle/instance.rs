//! Which identity instance a client document belongs to.

use serde::{Deserialize, Serialize};

/// The identity instance the continuity store attested when the client was
/// created: 32 lowercase hexadecimal characters, minted by the control plane
/// and never reused. A client id created again after its document was
/// removed carries a different one, or none, and so is never read as the
/// identity that existed before (O5).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct IdentityInstance(String);

impl IdentityInstance {
    /// The length of an instance, in hexadecimal characters.
    const LENGTH: usize = 32;

    /// An instance from its text.
    ///
    /// # Errors
    ///
    /// When `text` is not 32 lowercase hexadecimal characters.
    pub fn try_new(text: &str) -> Result<Self, String> {
        if text.len() == Self::LENGTH && text.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(text.to_owned()))
        } else {
            Err("an identity instance is 32 lowercase hexadecimal characters".to_owned())
        }
    }

    /// The text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for IdentityInstance {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        Self::try_new(&text)
    }
}

impl From<IdentityInstance> for String {
    fn from(instance: IdentityInstance) -> Self {
        instance.0
    }
}

impl std::fmt::Display for IdentityInstance {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
