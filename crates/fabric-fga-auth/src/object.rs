//! The thing a decision is about.

use std::fmt;

use fabric_core::LogicalResourceName;
use serde::{Deserialize, Serialize};

/// Characters that cannot appear in an object's identifier.
///
/// `:` separates the type from the id and `#` introduces a userset in the
/// authorization service's own syntax; `/` is this platform's separator. An id
/// carrying any of them would render as something other than itself.
const RESERVED: [char; 3] = [':', '#', '/'];

/// The longest object identifier this platform will carry.
const MAX_ID: usize = 255;

/// A resource instance: `customers:123`.
///
/// The type half is a [`LogicalResourceName`] — the same name the Data API's
/// catalogue uses and the same one a client's desired state declares its
/// relations against (ADR 0013). One spelling, checked by one rule, wherever a
/// resource is named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectRef {
    /// The resource type.
    resource: LogicalResourceName,

    /// The instance, as the caller's system names it.
    id: String,
}

impl ObjectRef {
    /// Parses `resource:id`.
    ///
    /// # Errors
    ///
    /// Returns a message naming the first rule broken. Objects arrive from a
    /// caller, so this runs before anything is sent onward — the
    /// authorization service accepts identifiers this platform should not.
    pub fn parse(value: &str) -> Result<Self, String> {
        let (resource, id) = value
            .split_once(':')
            .ok_or_else(|| "object must be written resource:id".to_owned())?;

        let resource =
            LogicalResourceName::try_new(resource).map_err(|error| format!("object type: {error}"))?;

        if id.is_empty() {
            return Err("object id must not be empty".to_owned());
        }

        if id.len() > MAX_ID {
            return Err(format!("object id must be at most {MAX_ID} characters"));
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

impl fmt::Display for ObjectRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.resource, self.id)
    }
}

impl<'de> Deserialize<'de> for ObjectRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;

        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

impl Serialize for ObjectRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
