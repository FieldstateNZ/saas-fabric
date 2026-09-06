//! Which kind of callback an application client is entitled to.

mod kind;
mod rules;
mod shape;
#[cfg(test)]
mod strategy_tests;

use crate::{DesiredStateError, RedirectUri};

use shape::RedirectStrategyShape;

pub use kind::RedirectStrategyKind;

/// The dotted path this type's refusals name.
const FIELD: &str = "spec.identity.clients";

/// What a client declares about where it may be sent back to.
///
/// # Why the fields are private
///
/// The kind and the URIs only mean anything together: a `claimedHttps`
/// strategy holding a loopback callback is the exact ambiguity this type
/// exists to remove, and a public field would let any caller assemble one.
/// [`Self::try_new`] is the only way to build one in Rust, and it applies the
/// same rules validation does.
///
/// Deserialisation is the deliberate exception. A stored document that breaks
/// a rule is refused by `validate` — with the dotted path an operator can find
/// in their own document — rather than by serde, whose message would be about
/// a Rust type. So the shape converts structurally and the rules run at the
/// three points `IdentityConfiguration::validate` runs at.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "RedirectStrategyShape", into = "RedirectStrategyShape")]
pub struct RedirectStrategy {
    /// Which kind of callback the client is entitled to.
    kind: RedirectStrategyKind,

    /// The callbacks themselves. Never empty in a valid document.
    uris: Vec<RedirectUri>,
}

impl RedirectStrategy {
    /// Builds a strategy, refusing any callback it would not admit.
    ///
    /// # Errors
    ///
    /// Returns [`DesiredStateError::InvalidField`] if the list is empty, if a
    /// URI's kind is not one this strategy admits, or if a wildcard stands
    /// where this strategy does not permit one.
    pub fn try_new(kind: RedirectStrategyKind, uris: Vec<RedirectUri>) -> Result<Self, DesiredStateError> {
        let strategy = Self { kind, uris };

        match rules::first_complaint(&strategy) {
            Some(detail) => Err(DesiredStateError::InvalidField { field: FIELD, detail }),
            None => Ok(strategy),
        }
    }

    /// Which kind of callback this client is entitled to.
    #[must_use]
    pub const fn kind(&self) -> &RedirectStrategyKind {
        &self.kind
    }

    /// The callbacks the identity provider should register.
    #[must_use]
    pub fn uris(&self) -> &[RedirectUri] {
        &self.uris
    }

    /// The first rule this strategy breaks, described for an operator.
    pub(crate) fn first_complaint(&self) -> Option<String> {
        rules::first_complaint(self)
    }

    /// Assembles a strategy without applying the rules.
    ///
    /// Private to this module tree, and used by exactly one caller: the serde
    /// shape. See the type's note on why a stored document is refused by
    /// `validate` rather than by deserialisation.
    pub(super) const fn from_parts(kind: RedirectStrategyKind, uris: Vec<RedirectUri>) -> Self {
        Self { kind, uris }
    }

    /// Takes the strategy apart again, for the serialise direction.
    pub(super) fn into_parts(self) -> (RedirectStrategyKind, Vec<RedirectUri>) {
        (self.kind, self.uris)
    }
}
