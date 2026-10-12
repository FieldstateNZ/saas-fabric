//! Which chain an audit record belongs to.

use fabric_client_model::ClientId;

/// One independently verifiable chain of audit entries.
///
/// # Why per stream, not one global chain
///
/// A chain per resource lets one client's history be verified, exported or
/// reasoned about without reading every other client's, and keeps appends to
/// unrelated resources from contending on one head. The streams are a closed
/// set, so a record cannot name a stream that does not exist.
///
/// # Its text form
///
/// `client.<id>`, `catalogue`, `registries` or `publication`. A client id is a
/// DNS label, so the text form is always lowercase ASCII letters, digits, `-`
/// and one `.` — safe as a file name, and never `.` or `..`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum AuditStream {
    /// Everything done to one client: creation, identity, product, secrets
    /// and lifecycle.
    Client(ClientId),
    /// The product catalogue, including component-version selection.
    Catalogue,
    /// The image registries operators register.
    Registries,
    /// Operator-triggered runtime-publication passes.
    Publication,
}

/// The prefix a client stream's text form carries.
const CLIENT_PREFIX: &str = "client.";

impl AuditStream {
    /// Parses the text form.
    ///
    /// # Errors
    ///
    /// A message if `text` names no stream.
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "catalogue" => Ok(Self::Catalogue),
            "registries" => Ok(Self::Registries),
            "publication" => Ok(Self::Publication),
            _ => text
                .strip_prefix(CLIENT_PREFIX)
                .ok_or_else(|| format!("`{text}` is not an audit stream"))
                .and_then(|id| ClientId::try_new(id).map_err(|error| error.to_string()))
                .map(Self::Client),
        }
    }
}

impl std::fmt::Display for AuditStream {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Client(id) => write!(formatter, "{CLIENT_PREFIX}{id}"),
            Self::Catalogue => formatter.write_str("catalogue"),
            Self::Registries => formatter.write_str("registries"),
            Self::Publication => formatter.write_str("publication"),
        }
    }
}

impl TryFrom<String> for AuditStream {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<AuditStream> for String {
    fn from(stream: AuditStream) -> Self {
        stream.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stream_round_trips_through_its_text_form() {
        let client = AuditStream::Client(ClientId::try_new("acme").unwrap());
        for stream in [
            client,
            AuditStream::Catalogue,
            AuditStream::Registries,
            AuditStream::Publication,
        ] {
            assert_eq!(AuditStream::parse(&stream.to_string()).unwrap(), stream);
        }
    }

    #[test]
    fn a_name_that_could_escape_a_directory_is_refused() {
        for text in [
            "..",
            ".",
            "client...",
            "client./etc",
            "client.",
            "Catalogue",
            "audit",
        ] {
            assert!(AuditStream::parse(text).is_err(), "{text} must be refused");
        }
    }
}
