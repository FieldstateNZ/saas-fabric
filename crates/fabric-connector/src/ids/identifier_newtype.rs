//! The shared shape of every validated name in this crate.

/// Declares a validated identifier newtype.
///
/// Five names in this crate need identical treatment: a checked constructor, a
/// `Display`, serde that validates on the way in, and the ordering and hashing
/// derives that let them be map keys. Writing that out five times would be
/// three hundred lines of copy-paste, and the copies would drift — one would
/// quietly lose its serde validation and nobody would notice until a malformed
/// name reached a query.
///
/// The macro is small and expands to exactly what you would have written by
/// hand. Each use site is a separate file with its own documentation, so the
/// types are still individually discoverable; only the boilerplate is shared.
///
/// All names use `fabric_core::naming::parse_identifier` — ASCII letters,
/// digits, hyphens and underscores, starting with a letter, at most 63 bytes.
macro_rules! identifier_newtype {
    ($(#[$meta:meta])* $name:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Parses the name, enforcing the shared identifier character set.
            ///
            /// # Errors
            ///
            /// Returns [`fabric_core::IdentifierError`] if the value is empty,
            /// longer than 63 bytes, does not start with an ASCII letter, or
            /// contains a character outside letters, digits, hyphens, and
            /// underscores.
            pub fn try_new(value: impl AsRef<str>) -> Result<Self, fabric_core::IdentifierError> {
                fabric_core::naming::parse_identifier($kind, value.as_ref()).map(Self)
            }

            /// Borrows the name as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl TryFrom<String> for $name {
            type Error = fabric_core::IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::try_new(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}
