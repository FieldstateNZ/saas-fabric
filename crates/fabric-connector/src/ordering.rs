//! Sort specifications.

/// Which way a field sorts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    /// Smallest first.
    Ascending,
    /// Largest first.
    Descending,
}

impl SortDirection {
    /// A stable name for telemetry and wire encoding.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ascending => "ascending",
            Self::Descending => "descending",
        }
    }
}

/// One element of a sort specification.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SortField {
    /// The field to sort on.
    pub field: crate::FieldName,
    /// Which direction.
    pub direction: SortDirection,
}

impl SortField {
    /// Sorts ascending by the given field.
    #[must_use]
    pub const fn ascending(field: crate::FieldName) -> Self {
        Self {
            field,
            direction: SortDirection::Ascending,
        }
    }

    /// Sorts descending by the given field.
    #[must_use]
    pub const fn descending(field: crate::FieldName) -> Self {
        Self {
            field,
            direction: SortDirection::Descending,
        }
    }
}
