//! Where the three objects live: a namespace this deployment states.

/// The namespace the runtime's `ConfigMap`s are written to.
///
/// Only the namespace is configurable. The three object names are ADR
/// 0018's, fixed, because the platform repository mounts them by name into
/// the runtime and a name that could vary is a name the two could disagree
/// on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationTarget {
    /// A Kubernetes namespace, used only inside this adapter.
    pub namespace: String,
}

impl PublicationTarget {
    /// Refuses a namespace that is not a DNS label, before it can become a
    /// path segment. The rule is `fabric-core`'s own, not a second copy of it.
    ///
    /// # Errors
    ///
    /// A message naming the rule, never the value's origin.
    pub fn validate(&self) -> Result<(), String> {
        fabric_core::naming::parse_dns_label("namespace", &self.namespace)
            .map(|_| ())
            .map_err(|_| "publication requires a DNS-label namespace".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dns_label_namespace_is_accepted_and_others_are_refused() {
        for good in ["platform-system", "a", "ns-1"] {
            assert!(
                PublicationTarget {
                    namespace: good.into()
                }
                .validate()
                .is_ok(),
                "{good}"
            );
        }
        for bad in ["", "-x", "x-", "Platform", "a/b", "a b"] {
            assert!(
                PublicationTarget {
                    namespace: bad.into()
                }
                .validate()
                .is_err(),
                "{bad}"
            );
        }
    }
}
