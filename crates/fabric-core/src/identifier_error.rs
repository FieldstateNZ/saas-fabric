//! The single error type produced when an identifier fails to parse.

/// Why a string could not be turned into one of the platform's validated
/// identifier newtypes.
///
/// Identifier parsing is the platform's first line of defence: a `TenantId`
/// that exists has already been proven safe to interpolate into a schema name,
/// a pool key, and a metrics label. Every variant here therefore describes a
/// value that was rejected *before* it reached any of those places.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentifierError {
    /// The value was empty or contained only whitespace.
    #[error("{kind} must not be empty")]
    Empty {
        /// The identifier type that rejected the value, for example `tenant id`.
        kind: &'static str,
    },

    /// The value exceeded the maximum permitted length.
    #[error("{kind} must be at most {max} characters, got {actual}")]
    TooLong {
        /// The identifier type that rejected the value.
        kind: &'static str,
        /// The inclusive maximum length in bytes.
        max: usize,
        /// The length of the offending value in bytes.
        actual: usize,
    },

    /// The value contained a character outside the permitted set.
    ///
    /// The offending character is reported rather than the whole value, because
    /// identifiers can originate from a bearer token and echoing the full value
    /// back into logs or an error body risks reflecting attacker-controlled
    /// content.
    #[error("{kind} contains disallowed character {character:?}; expected {expected}")]
    DisallowedCharacter {
        /// The identifier type that rejected the value.
        kind: &'static str,
        /// The first character that was not permitted.
        character: char,
        /// A human-readable description of the permitted character set.
        expected: &'static str,
    },

    /// The value started or ended with a character that is only valid in the
    /// interior of the identifier, such as a leading hyphen.
    #[error("{kind} must start and end with an alphanumeric character")]
    BadBoundary {
        /// The identifier type that rejected the value.
        kind: &'static str,
    },

    /// The value is well-formed and still outside the boundary a rule draws
    /// for reasons that have nothing to do with its characters or its shape.
    ///
    /// Its own variant rather than a [`Self::BadBoundary`] because the message
    /// is the whole point: a value can be syntactically fine — every
    /// character permitted, correctly bounded — and still fail a rule that is
    /// about something else entirely, such as which scheme it declares, which
    /// network it names, or whether a required part is present at all.
    /// Telling its author that it "must start and end with an alphanumeric
    /// character" sends them hunting for a typo in a value that has none.
    /// What is wrong is the boundary, so the boundary is what gets named, in
    /// the author's own terms.
    ///
    /// For example, `127.0.0.2` reaches loopback on every operating system and
    /// is not one of the three spellings this platform recognises as
    /// loopback; the message names that boundary directly rather than reading
    /// as a parse failure.
    #[error("{kind} does not admit this value: {expected}")]
    Unadmitted {
        /// The identifier type that rejected the value.
        kind: &'static str,
        /// The boundary the value fell outside, in the author's own terms.
        expected: &'static str,
    },
}
