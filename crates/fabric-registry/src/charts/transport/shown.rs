//! Rendering a URL for a message that might reach an operator or a log.

/// Renders `url` down to what a reader needs to tell one chart repository
/// address from another: scheme, host, port when it differs from the
/// scheme's default, and path. Nothing else.
///
/// # Why not just format the [`Url`](reqwest::Url) itself
///
/// A chart repository address is whatever a caller configured, and
/// `reqwest::Url`'s own `Display` renders it exactly as written — userinfo
/// included. `https://user:secret@host/index.yaml` would put `secret`
/// straight into any message built from it directly. Every message this
/// crate raises about a URL ends up in [`RegistryError::Refused`], which the
/// control-plane API returns to the console verbatim and which a sweep
/// failure logs, so a credential embedded in a repository address must never
/// reach either. The query and fragment are dropped for the same reason —
/// either can carry a token as easily as userinfo carries a password, and
/// neither tells a reader anything about *where* the request went that the
/// path does not already say.
///
/// Every URL this crate names in a message should be rendered through this
/// function rather than formatted directly, so the guarantee is one rule
/// applied everywhere a URL might be shown, not a judgement call repeated at
/// every call site.
///
/// [`RegistryError::Refused`]: fabric_platform_management::RegistryError::Refused
#[must_use]
pub(in crate::charts) fn shown(url: &reqwest::Url) -> String {
    use std::fmt::Write as _;

    let mut rendered = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));

    // `Url::port` already answers "non-default": it is `None` unless the URL
    // named a port that differs from the scheme's own, so there is no
    // separate default-port table to keep in step with `reqwest`'s. Writing
    // into the `String` in place, rather than building and appending another
    // `format!`, avoids a throwaway allocation for a couple of digits.
    if let Some(port) = url.port() {
        let _ = write!(rendered, ":{port}");
    }

    rendered.push_str(url.path());
    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn userinfo_query_and_fragment_never_reach_the_rendering() {
        let url = reqwest::Url::parse("https://user:secret@example.test:8443/repo/index.yaml?x=y#z")
            .expect("test URL parses");

        let rendered = shown(&url);

        assert!(!rendered.contains("user"), "{rendered}");
        assert!(!rendered.contains("secret"), "{rendered}");
        assert!(!rendered.contains("x=y"), "{rendered}");
        assert!(!rendered.contains('z'), "{rendered}");
        assert!(
            rendered.contains("example.test:8443/repo/index.yaml"),
            "{rendered}"
        );
    }

    #[test]
    fn a_default_port_is_left_out() {
        let url = reqwest::Url::parse("https://example.test/index.yaml").expect("test URL parses");

        assert_eq!(shown(&url), "https://example.test/index.yaml");
    }
}
