//! The proof-key method a public client must perform.

/// The PKCE code-challenge method an application client is required to use.
///
/// # Why a single-variant enum, and why `plain` is not one of them
///
/// Every client SaaS Fabric declares is a *public* client, which means its
/// authorisation code travels back through a browser or an operating system
/// with no client secret protecting the exchange. PKCE is what stops an
/// intercepted code being redeemable by whoever intercepted it, and RFC 8252
/// §8.1 requires it for exactly that reason.
///
/// `plain` sends the verifier itself as the challenge, so anything that could
/// intercept the code could also intercept the proof. It is deliberately not a
/// variant: a document naming it fails to deserialise, which makes the refusal
/// a property of the type rather than a rule some validator has to remember.
/// There is no code path, now or later, that can build a client requiring it.
///
/// Single-variant rather than absent follows [`ClientProtocol`]'s precedent
/// (see `oidc_client.rs`): the document says the method out loud, so a future
/// method is an added variant that old documents keep parsing — where
/// inferring it would mean every existing document silently acquiring a
/// meaning it never stated.
///
/// [`ClientProtocol`]: crate::ClientProtocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PkceMethod {
    /// SHA-256, as RFC 7636 §4.2 defines it. The only method this platform
    /// will ever write.
    S256,
}

impl PkceMethod {
    /// The spelling the identity provider holds.
    ///
    /// Deliberately not the same string as the document's `s256`: RFC 7636
    /// names the method `S256`, and that upper-case form is what an
    /// authorization request carries. Keeping the one translation here means
    /// the adapter that writes it and the reconciler that compares what came
    /// back cannot disagree about the spelling — a disagreement that would
    /// present as a client permanently drifted and rewritten on every sweep.
    #[must_use]
    pub const fn as_wire_value(self) -> &'static str {
        match self {
            Self::S256 => "S256",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_pkce_method_is_not_a_value_this_model_can_hold() {
        let error = serde_norway::from_str::<PkceMethod>("plain").unwrap_err();

        assert!(
            error.to_string().contains("plain") && error.to_string().contains("s256"),
            "{error}"
        );
    }

    #[test]
    fn the_challenge_method_has_one_spelling() {
        // The document says `s256`; RFC 7636 and the identity provider say
        // `S256`. One translation, in one place, so the writer and the
        // comparer cannot drift apart.
        assert_eq!(PkceMethod::S256.as_wire_value(), "S256");
        assert_eq!(serde_norway::to_string(&PkceMethod::S256).unwrap().trim(), "s256");
    }
}
