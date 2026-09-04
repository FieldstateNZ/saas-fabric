//! Rendering a URL for a message that might reach an operator or a log.

/// How much of a path this will show.
///
/// A path is server- or caller-controlled, not authored by this crate — a
/// redirect target's path is whatever the far end wrote it as, and an index
/// address's path is whatever a caller pasted. Matching
/// `SafeDiagnostic::MAX`'s reasoning: a console line, not a log, so the cap
/// is what survives a value nobody bounded, and a 200 000-character path
/// becomes a marker and 200 characters instead of a 200 KB console line and
/// log line.
const MAX_PATH_CHARS: usize = 200;

/// Renders `url` down to what a reader needs to tell one chart repository
/// address from another: scheme, host, port when it differs from the
/// scheme's default, and path, capped at [`MAX_PATH_CHARS`]. Nothing else.
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
/// # A cannot-be-a-base URL is handled before anything else
///
/// `oci:user:secret@registry.example/charts` and `mailto:user:secret@x`
/// parse without a `//` after the scheme, so `url` treats everything after
/// the scheme as one opaque, unstructured blob: `host_str`, `username`,
/// `password`, `query`, and `fragment` all report empty or absent, not
/// because there is no credential, but because there is no structure for a
/// credential to be part of. Formatting such a URL the same way as a
/// hierarchical one — scheme, host, path — would still print that entire
/// blob under the label "path", credential included. So this checks
/// [`Url::cannot_be_a_base`](reqwest::Url::cannot_be_a_base) *first*, before
/// looking at any other part of the URL, and renders one of those addresses
/// as only its scheme.
///
/// Every URL this crate names in a message should be rendered through this
/// function rather than formatted directly, so the guarantee is one rule
/// applied everywhere a URL might be shown, not a judgement call repeated at
/// every call site — including a redirect target, which reaches this
/// function no matter its shape (see [`validated_index_url`]'s docs on why
/// refusing a cannot-be-a-base address up front does not, by itself, cover
/// that case).
///
/// The path itself is not treated as secret — it names which repository and
/// which chart, which is exactly what a reader needs — so it is capped for
/// *length* only, never redacted. A future integration that can legitimately
/// put a token in a URL's path inherits that choice knowingly by reading
/// this sentence, not by accident.
///
/// [`RegistryError::Refused`]: fabric_platform_management::RegistryError::Refused
/// [`validated_index_url`]: super::validated_index_url
#[must_use]
pub(in crate::charts) fn shown(url: &reqwest::Url) -> String {
    use std::fmt::Write as _;

    if url.cannot_be_a_base() {
        return format!("{}:[opaque]", url.scheme());
    }

    let mut rendered = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));

    // `Url::port` already answers "non-default": it is `None` unless the URL
    // named a port that differs from the scheme's own, so there is no
    // separate default-port table to keep in step with `reqwest`'s. Writing
    // into the `String` in place, rather than building and appending another
    // `format!`, avoids a throwaway allocation for a couple of digits.
    if let Some(port) = url.port() {
        let _ = write!(rendered, ":{port}");
    }

    let path = url.path();
    match path.char_indices().nth(MAX_PATH_CHARS) {
        None => rendered.push_str(path),
        Some((cut, _)) => {
            rendered.push_str(path.get(..cut).unwrap_or_default());
            rendered.push('…');
        }
    }

    rendered
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One rendering per shape this crate has to render safely: a
    /// credential-and-tracking-bearing hierarchical URL, two cannot-be-a-base
    /// schemes that carry a credential with no structure to catch it in, a
    /// `file:` URL with an empty-but-present host, an IPv6 host, and a
    /// default port left out beside a non-default one kept.
    #[test]
    fn renders_only_what_is_safe_to_show() {
        let cases: &[(&str, &str)] = &[
            (
                "https://user:secret@example.test:8443/repo/index.yaml?x=y#z",
                "https://example.test:8443/repo/index.yaml",
            ),
            ("oci:user:secret@registry.example/charts", "oci:[opaque]"),
            ("mailto:user:secret@example.test", "mailto:[opaque]"),
            ("file:///charts/index.yaml", "file:///charts/index.yaml"),
            ("https://[::1]:8080/index.yaml", "https://[::1]:8080/index.yaml"),
            (
                "https://example.test:443/index.yaml",
                "https://example.test/index.yaml",
            ),
        ];

        for (input, expected) in cases {
            let url = reqwest::Url::parse(input).expect("test URL parses");
            assert_eq!(&shown(&url), expected, "rendering {input}");
        }
    }

    #[test]
    fn a_long_path_is_capped_rather_than_shown_whole() {
        // A redirect target's path is whatever the far end wrote, and
        // nothing here bounds how long that could be before this does.
        let long_path = "a".repeat(500);
        let url = reqwest::Url::parse(&format!("https://example.test/{long_path}")).expect("test URL parses");

        let rendered = shown(&url);

        assert!(rendered.len() < 300, "{}", rendered.len());
        assert!(rendered.ends_with('…'), "{rendered}");
        assert!(rendered.starts_with("https://example.test/aaa"), "{rendered}");
    }
}
