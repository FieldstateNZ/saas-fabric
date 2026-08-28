//! The shared shape of the names that reuse a `fabric_core::naming` rule.

/// Declares a validated identifier newtype over one of the shared naming rules.
///
/// Three names in this crate — a client id, a realm name, an OIDC client id —
/// need the same treatment: a checked constructor, a `Display`, serde that
/// validates on the way in, and the ordering derives that let them be map keys
/// and sort deterministically in a rendered document. The macro expands to
/// exactly what each would otherwise repeat, and each use site keeps its own
/// file and its own documentation, so the types stay individually
/// discoverable.
///
/// The rule is a parameter rather than fixed, because the three do not share
/// one: a realm name becomes a URL path segment and a Kubernetes-ish label, so
/// it takes the strict DNS rule; an OIDC client id is written by a platform
/// engineer and may carry an underscore.
///
/// The names that are *not* declared here — [`RoleName`](super::RoleName),
/// [`Host`](super::Host), [`ClientRevision`](super::ClientRevision) — have
/// genuinely different rules and are written out by hand rather than bent to
/// fit this shape.
macro_rules! slug_newtype {
    ($(#[$meta:meta])* $name:ident, $kind:literal, $parse:path) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Parses the name, enforcing its character set.
            ///
            /// # Errors
            ///
            /// Returns [`fabric_core::IdentifierError`] describing the first
            /// rule the value broke.
            pub fn try_new(value: impl AsRef<str>) -> Result<Self, fabric_core::IdentifierError> {
                $parse($kind, value.as_ref()).map(Self)
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
