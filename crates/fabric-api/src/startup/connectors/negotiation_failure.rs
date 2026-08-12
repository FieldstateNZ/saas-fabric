//! The negotiation failure, as an error that can travel in a source chain.

/// Wraps a negotiation failure message as a [`std::error::Error`], so it can
/// travel inside [`ConnectorError::Unreachable`]'s source.
#[derive(Debug)]
pub(super) struct NegotiationFailure(pub(super) String);

impl std::fmt::Display for NegotiationFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for NegotiationFailure {}
