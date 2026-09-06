//! Which strategy a `v1` client's flat callback list reads as.
//!
//! The whole of the migrator's judgement, in one place: five arms, of which
//! two are refusals. Split from the traversal because the traversal is
//! mechanical and this is the decision.

use serde_norway::Value;

use super::{FIELD, REPLACEMENT};
use crate::{DesiredStateError, RedirectUri, RedirectUriKind};

/// The one strategy every callback in a `v1` list agrees on.
///
/// A mix is refused rather than resolved because there is no honest
/// resolution: a client holding both a production callback and a loopback one
/// is the exact ambiguity the strategy exists to remove, and picking the
/// looser one would silently grant an entitlement the operator never stated.
///
/// The private-use arm cannot arise from a document `v1` could hold — those
/// schemes did not parse before this change — and is written anyway, so the
/// migrator stays total now that `RedirectUri` admits them.
///
/// # Errors
///
/// Returns [`DesiredStateError::Migration`] for a mixed list or a private-use
/// scheme, and [`DesiredStateError::Malformed`] for a callback that does not
/// parse at all.
pub(super) fn of(uris: &Value) -> Result<&'static str, DesiredStateError> {
    let entries: &[Value] = uris.as_sequence().map_or(&[], Vec::as_slice);
    let mut agreed: Option<RedirectUriKind> = None;

    for entry in entries {
        let kind = classify(entry.as_str().unwrap_or_default())?;

        match agreed {
            Some(seen) if seen != kind => return Err(mixed()),
            _ => agreed = Some(kind),
        }
    }

    match agreed {
        Some(RedirectUriKind::Https) => Ok("claimedHttps"),
        Some(RedirectUriKind::PrivateNetwork) => Ok("privateNetwork"),
        // An empty list keeps `v1`'s own refusal rather than acquiring a new
        // one: `development` holds it until validation says a client with no
        // callback could never sign anyone in.
        Some(RedirectUriKind::Loopback) | None => Ok("development"),
        Some(RedirectUriKind::PrivateUseScheme) => Err(private_use()),
    }
}

/// What kind one stored callback is, or why it cannot be read at all.
fn classify(text: &str) -> Result<RedirectUriKind, DesiredStateError> {
    RedirectUri::try_new(text)
        .map(|uri| uri.kind())
        .map_err(|error| DesiredStateError::Malformed {
            detail: format!("{FIELD}: {error}"),
        })
}

/// The refusal a mixed list produces.
fn mixed() -> DesiredStateError {
    DesiredStateError::Migration {
        field: FIELD,
        replacement: REPLACEMENT,
        detail: "its callbacks are not all of one kind, so no strategy describes this client \
                 honestly; migrate it to v2 by hand, naming the entitlement it should have"
            .to_owned(),
    }
}

/// The refusal a private-use scheme produces.
///
/// It names **both** readings, because the classifier cannot tell them apart
/// and only the operator can. `nz.fieldstate.slipway:/cb` is a native
/// application's scheme; `www.example.com:/cb` is a host that lost its
/// `https://`, and it reaches this arm because a forward domain is as
/// syntactically valid a scheme as a reversed one. (The spelling with a port,
/// `www.example.com:8080/cb`, does not reach here at all — the digit after the
/// colon makes it a parse failure, which is a sharper answer.) Telling an
/// operator only the first would send the author of the second to write a
/// `customScheme` strategy for a typo.
fn private_use() -> DesiredStateError {
    DesiredStateError::Migration {
        field: FIELD,
        replacement: REPLACEMENT,
        detail: "it declares something this model reads as a private-use scheme, which v1 had no \
                 way to describe. If that is what it is — a native application's own scheme, in \
                 reverse-domain form — migrate the client to v2 by hand under the customScheme \
                 strategy. If it is a host that lost its https:// prefix, such as \
                 www.example.com:/cb, write the scheme instead"
            .to_owned(),
    }
}
