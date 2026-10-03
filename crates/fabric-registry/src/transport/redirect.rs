//! Wiring a reader's transport decision into `reqwest`'s own redirect policy.

/// Carries a refusal reason through `reqwest`'s own redirect-error channel,
/// so a caller can recognise a policy refusal via `reqwest::Error::is_redirect`
/// without this crate inventing a second channel for the same information.
#[derive(Debug)]
struct RedirectRefused(String);

impl std::fmt::Display for RedirectRefused {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RedirectRefused {}

/// Builds a redirect policy from `decide`, which is handed the hops already
/// followed and the next URL, and answers the refusal's wording when it says
/// no.
///
/// The wording is the reader's own, because readers disagree about what may
/// be shown: a chart repository's redirect target is shown through
/// [`shown`](super::shown()), while a registry's is never shown at all — a
/// blob's CDN target carries a signed query, and its path is no business of
/// a console either.
pub(crate) fn policy<F>(decide: F) -> reqwest::redirect::Policy
where
    F: Fn(&[reqwest::Url], &reqwest::Url) -> Result<(), String> + Send + Sync + 'static,
{
    reqwest::redirect::Policy::custom(move |attempt| match decide(attempt.previous(), attempt.url()) {
        Ok(()) => attempt.follow(),
        Err(reason) => attempt.error(RedirectRefused(reason)),
    })
}
