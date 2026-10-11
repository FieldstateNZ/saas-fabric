//! The number a registry generation is stamped with.

/// Which generation of the issuer registry this is.
///
/// Starts at 1 and advances by one each time the registrations change; a
/// publication that leaves them alone leaves the generation alone. It never
/// goes backwards and is never reused for different registrations, so an
/// edge that states the generation it applied can be held to exactly one set
/// of issuers (D01-4). Distinct from the publication record's own generation,
/// which also advances when only a document changes: tying the edge to that
/// number would make every catalogue change look like an identity change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u64", into = "u64")]
pub struct RegistryGeneration(u64);

impl RegistryGeneration {
    /// The first generation.
    pub const FIRST: Self = Self(1);

    /// The generation after this one.
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }

    /// The number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A registry generation of zero, which no registry is ever stamped with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a registry generation starts at 1")]
pub struct ZeroGeneration;

impl TryFrom<u64> for RegistryGeneration {
    type Error = ZeroGeneration;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value == 0 {
            Err(ZeroGeneration)
        } else {
            Ok(Self(value))
        }
    }
}

impl From<RegistryGeneration> for u64 {
    fn from(generation: RegistryGeneration) -> Self {
        generation.0
    }
}

impl std::fmt::Display for RegistryGeneration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}
