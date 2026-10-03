//! The id that names a registry's credential in the secret partition.

use std::fmt::Write as _;

use crate::git_integration::SecretName;

/// An opaque id this server minted, used only to name a credential in the
/// secret partition.
///
/// # Why minted, and why checked on the way back in
///
/// The secret store builds a location from a name, and a name built from
/// anything a request carried — a host, say — would let that request choose
/// where a credential is written. This is sixteen random hexadecimal
/// characters and nothing else, checked again when a record is read, so even
/// a record somebody edited by hand cannot name `../git/app-private-key`.
///
/// A new one is minted whenever a credential is set, so a replacement is
/// written beside the old one and the record is what switches between them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct SecretId(String);

/// How many random bytes an id carries: sixteen hexadecimal characters.
const ID_BYTES: usize = 8;

impl SecretId {
    /// Mints an id.
    ///
    /// # Errors
    ///
    /// A message if the system's randomness is unavailable. Failing is the
    /// only option: a predictable id is one another write could collide with.
    pub(crate) fn mint() -> Result<Self, String> {
        let mut bytes = [0_u8; ID_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| "the system's randomness is unavailable".to_owned())?;
        Ok(Self(bytes.iter().fold(String::new(), |mut hex, byte| {
            // Writing to a `String` cannot fail.
            let _ = write!(hex, "{byte:02x}");
            hex
        })))
    }

    /// The name of the credential this id stands for.
    pub(crate) fn credential(&self) -> SecretName {
        SecretName::new(format!("integrations/registries/{}/credential", self.0))
    }
}

impl TryFrom<String> for SecretId {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let well_formed = value.len() == ID_BYTES * 2
            && value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if well_formed {
            Ok(Self(value))
        } else {
            Err("a registry's secret id is sixteen lower-case hexadecimal characters".to_owned())
        }
    }
}

impl From<SecretId> for String {
    fn from(value: SecretId) -> Self {
        value.0
    }
}
