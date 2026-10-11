//! The object a relation Check is about.

use std::fmt;

use fabric_core::LogicalResourceName;

/// Characters that cannot appear in an object identifier.
///
/// The same three the authorization front refuses (`fabric-fga-auth`'s
/// `ObjectRef`): `:` separates type from id, `#` introduces a userset, and `/`
/// is the platform's separator. Repeated here rather than shared because this
/// crate has no edge to the trust boundary (crate-dependencies.md); the front
/// re-checks, so this copy only decides *where* an unusable key is refused.
const RESERVED: [char; 3] = [':', '#', '/'];

/// The longest object identifier the front accepts.
const MAX_ID: usize = 255;

/// One record of a resource: `customers:42`.
///
/// Built from the catalogue's resource name and the key in the request path.
/// A key that cannot name an object is refused here, as a denial, rather than
/// sent to the front to come back as a malformed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckObject {
    /// The resource type, spelled as desired state declares it (ADR 0013).
    resource: LogicalResourceName,

    /// The record's key.
    id: String,
}

impl CheckObject {
    /// Names a record as an object.
    ///
    /// # Errors
    ///
    /// Returns a message naming the rule the key breaks.
    pub fn try_new(resource: LogicalResourceName, id: &str) -> Result<Self, String> {
        if id.is_empty() {
            return Err("object id must not be empty".to_owned());
        }

        if id.len() > MAX_ID {
            return Err(format!("object id must be at most {MAX_ID} bytes"));
        }

        if let Some(bad) = id.chars().find(|c| c.is_whitespace() || RESERVED.contains(c)) {
            return Err(format!("object id must not contain {bad:?}"));
        }

        Ok(Self {
            resource,
            id: id.to_owned(),
        })
    }

    /// The resource type.
    #[must_use]
    pub const fn resource(&self) -> &LogicalResourceName {
        &self.resource
    }
}

impl fmt::Display for CheckObject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.resource, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn customers() -> LogicalResourceName {
        LogicalResourceName::try_new("customers").unwrap()
    }

    #[test]
    fn a_plain_key_renders_as_resource_and_id() {
        assert_eq!(
            CheckObject::try_new(customers(), "42").unwrap().to_string(),
            "customers:42"
        );
    }

    #[test]
    fn a_key_with_a_separator_cannot_name_an_object() {
        for key in ["a:b", "a#b", "a/b", "a b", ""] {
            assert!(CheckObject::try_new(customers(), key).is_err(), "{key:?}");
        }
    }

    #[test]
    fn an_overlong_key_cannot_name_an_object() {
        assert!(CheckObject::try_new(customers(), &"k".repeat(MAX_ID + 1)).is_err());
        assert!(CheckObject::try_new(customers(), &"k".repeat(MAX_ID)).is_ok());
    }
}
