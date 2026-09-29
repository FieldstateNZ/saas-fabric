//! The shared shape of the component descriptor's validated strings.

/// Declares a validated string newtype over a rule `$check`, a
/// `fn(&str) -> Result<(), ContractError>`.
///
/// Every name a component descriptor carries needs the same treatment: a
/// checked constructor, `as_str`, `Display`, serde that validates on the way
/// in, and the ordering derives that let a value be a map key and sort
/// deterministically in canonical bytes. Only that boilerplate is shared;
/// each type keeps its own file, its own rule and its own documentation.
macro_rules! contract_newtype {
    ($(#[$meta:meta])* $name:ident, $check:path) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Parses the value, enforcing its rule.
            ///
            /// # Errors
            ///
            /// Returns [`ContractError::Invalid`](crate::ContractError::Invalid)
            /// describing the rule the value broke.
            pub fn try_new(value: impl AsRef<str>) -> Result<Self, crate::ContractError> {
                let value = value.as_ref();
                $check(value)?;
                Ok(Self(value.to_owned()))
            }

            /// Borrows the value as a string slice.
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
            type Error = crate::ContractError;

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
