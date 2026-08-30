//! Where a secret sits inside a client's boundary.

use std::fmt;

/// The longest path this platform will address.
const MAX_LENGTH: usize = 255;

/// A path within one client's boundary — `database/primary`, `smtp`.
///
/// # What it refuses, and why each one matters
///
/// This is the one value in a secret request that a caller does supply, so it
/// is the one that has to be checked. A path is refused if it is empty, is
/// absolute, contains a traversal segment, or carries anything outside a
/// narrow set — because every one of those is a way to address something other
/// than what it appears to.
///
/// `..` is the sharp one. The boundary is enforced by prefixing a namespace,
/// and a path that can climb out of its prefix makes that enforcement
/// decorative.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SecretPath(String);

impl SecretPath {
    /// Parses a path within a boundary.
    ///
    /// # Errors
    ///
    /// Returns a message naming the first rule broken. The message repeats the
    /// rule rather than the value, so a rejected path is not echoed back into
    /// a log or a page.
    pub fn parse(value: &str) -> Result<Self, String> {
        if value.is_empty() {
            return Err("a secret path must not be empty".to_owned());
        }

        if value.len() > MAX_LENGTH {
            return Err(format!("a secret path must be at most {MAX_LENGTH} characters"));
        }

        if value.starts_with('/') || value.ends_with('/') {
            return Err(
                "a secret path is relative to the client, so it cannot begin or end with '/'".to_owned(),
            );
        }

        for segment in value.split('/') {
            if segment.is_empty() {
                return Err("a secret path must not contain an empty segment".to_owned());
            }

            if segment == "." || segment == ".." {
                return Err("a secret path must not contain '.' or '..'".to_owned());
            }

            if !segment.chars().all(is_permitted) {
                return Err("a secret path may contain letters, digits, '-', '_', '.' and '/'".to_owned());
            }
        }

        Ok(Self(value.to_owned()))
    }

    /// Borrows the path as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether a character may appear in a path segment.
///
/// Deliberately narrow. A percent sign would let a caller encode a separator
/// the store decodes later, which is the same escape by a slower route.
fn is_permitted(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
}

impl fmt::Display for SecretPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
