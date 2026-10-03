//! What a component needs from the platform.
use serde::{Deserialize, Serialize};

/// A platform capability: ADR 0021's closed list of seven, now a type.
///
/// Serialized as the exact words the catalogue has always stored for a
/// `capability` component's reference -- `Object storage` with its space
/// included -- so the catalogue and a component descriptor name a capability
/// the same way.
///
/// # Why the list is closed
///
/// A component descriptor is published and cannot be edited, and a reader
/// must answer for every value it can hold. A new capability is therefore a
/// new component descriptor version (ADR 0026 section 2), never a new variant
/// read under v1: an older build would otherwise refuse a document it claims
/// to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PlatformCapability {
    /// Sign-in and the identities a component's users hold.
    #[serde(rename = "Identity")]
    Identity,
    /// A database the component's data lives in.
    #[serde(rename = "Database")]
    Database,
    /// Secret storage.
    #[serde(rename = "Secrets")]
    Secrets,
    /// Authorization decisions.
    #[serde(rename = "Authorization")]
    Authorization,
    /// Traffic routing to the component.
    #[serde(rename = "Routing")]
    Routing,
    /// Object storage.
    #[serde(rename = "Object storage")]
    ObjectStorage,
    /// Messaging between components.
    #[serde(rename = "Messaging")]
    Messaging,
}

impl PlatformCapability {
    /// Every capability, in the order ADR 0021 lists them.
    pub const ALL: [Self; 7] = [
        Self::Identity,
        Self::Database,
        Self::Secrets,
        Self::Authorization,
        Self::Routing,
        Self::ObjectStorage,
        Self::Messaging,
    ];

    /// The capability's serialized name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Identity => "Identity",
            Self::Database => "Database",
            Self::Secrets => "Secrets",
            Self::Authorization => "Authorization",
            Self::Routing => "Routing",
            Self::ObjectStorage => "Object storage",
            Self::Messaging => "Messaging",
        }
    }

    /// The capability `value` names exactly, if any. Case and spacing are
    /// significant, as they are in a stored catalogue.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|capability| capability.as_str() == value)
    }
}

impl std::fmt::Display for PlatformCapability {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_to_exactly_the_seven_names() {
        let names: Vec<String> = PlatformCapability::ALL
            .iter()
            .map(|capability| serde_json::to_string(capability).unwrap())
            .collect();

        assert_eq!(
            names,
            [
                "\"Identity\"",
                "\"Database\"",
                "\"Secrets\"",
                "\"Authorization\"",
                "\"Routing\"",
                "\"Object storage\"",
                "\"Messaging\"",
            ]
        );
    }

    #[test]
    fn as_str_parse_and_serde_agree() {
        for capability in PlatformCapability::ALL {
            assert_eq!(PlatformCapability::parse(capability.as_str()), Some(capability));
            let text = serde_json::to_string(&capability).unwrap();
            assert_eq!(text, format!("\"{capability}\""));
            assert_eq!(
                serde_json::from_str::<PlatformCapability>(&text).unwrap(),
                capability
            );
        }
    }

    #[test]
    fn a_near_miss_is_not_a_capability() {
        assert_eq!(PlatformCapability::parse("database"), None);
        assert_eq!(PlatformCapability::parse("ObjectStorage"), None);
        assert_eq!(PlatformCapability::parse("Object Storage"), None);
        assert!(serde_json::from_str::<PlatformCapability>("\"Queues\"").is_err());
    }
}
