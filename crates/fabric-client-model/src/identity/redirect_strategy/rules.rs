//! Which callbacks each strategy admits, and which wildcards.
//!
//! The parser widens universally and these rules narrow. Keeping the widening
//! in `redirect_uri` and the entitlement here is what stops the parser growing
//! a second copy of this table — and it is why a URI outside its strategy is
//! **refused**, naming what the strategy admits, rather than reclassified into
//! a strategy that would take it.

use super::{RedirectStrategy, RedirectStrategyKind};
use crate::{RedirectUri, RedirectUriKind};

/// The first rule this strategy breaks, described for an operator.
///
/// One complaint rather than all of them, following the rest of this model:
/// the message names something the operator can find in their own document,
/// and a second complaint about the same block is noise until the first is
/// fixed.
pub(super) fn first_complaint(strategy: &RedirectStrategy) -> Option<String> {
    if strategy.uris().is_empty() {
        return Some("declares no redirect URI, so it could never sign a user in".to_owned());
    }

    strategy
        .uris()
        .iter()
        .find_map(|uri| complaint_about(strategy.kind(), uri))
}

/// What is wrong with one callback under one strategy, if anything.
fn complaint_about(kind: &RedirectStrategyKind, uri: &RedirectUri) -> Option<String> {
    if !admits(kind, uri) {
        return Some(format!(
            "{uri} is {} and the {kind} strategy admits {}",
            uri.kind(),
            admitted(kind)
        ));
    }

    if uri.has_path_wildcard() && !matches!(kind, RedirectStrategyKind::Development) {
        return Some(format!(
            "{uri} carries a path wildcard, which the {kind} strategy does not admit: RFC 9700 \
             §2.1 requires a redirect URI to be matched exactly, and a Universal Link or App Link \
             needs an exact URL in any case"
        ));
    }

    if uri.has_wildcard_port() && !matches!(kind, RedirectStrategyKind::Development) {
        return Some(format!(
            "{uri} names every port, which only the development strategy admits: a wildcard port \
             on any other host is a redirect URI matching every port on it"
        ));
    }

    None
}

/// Whether a strategy admits a callback of this kind.
///
/// A private-use scheme is admitted only by the strategy declaring the *same*
/// scheme. Anything else would let one application's callback be registered
/// against another's, which is the interception RFC 8252 §8.6 warns about.
fn admits(kind: &RedirectStrategyKind, uri: &RedirectUri) -> bool {
    match (kind, uri.kind()) {
        (RedirectStrategyKind::ClaimedHttps, RedirectUriKind::Https)
        | (RedirectStrategyKind::PrivateNetwork, RedirectUriKind::PrivateNetwork)
        | (RedirectStrategyKind::Development, RedirectUriKind::Loopback) => true,
        (RedirectStrategyKind::CustomScheme(declared), RedirectUriKind::PrivateUseScheme) => uri
            .as_str()
            .split_once(':')
            .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case(declared.as_str())),
        _ => false,
    }
}

/// What a strategy admits, for the message a refusal produces.
fn admitted(kind: &RedirectStrategyKind) -> &'static str {
    match kind {
        RedirectStrategyKind::ClaimedHttps => "only public https callbacks",
        RedirectStrategyKind::PrivateNetwork => "only .internal callbacks, over http or https",
        RedirectStrategyKind::Development => "only loopback callbacks — 127.0.0.1, ::1 or localhost",
        RedirectStrategyKind::CustomScheme(_) => "only callbacks on the scheme it declares",
    }
}
