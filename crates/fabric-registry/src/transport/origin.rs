//! Where a URL points: its origin, and whether that is loopback or an address.

/// Whether two URLs share an origin: scheme, host and port, a port left
/// implicit being the scheme's own.
pub(crate) fn same_origin(one: &reqwest::Url, other: &reqwest::Url) -> bool {
    one.scheme() == other.scheme()
        && one.host_str() == other.host_str()
        && one.port_or_known_default() == other.port_or_known_default()
}

/// Whether a URL's host is loopback: `127.0.0.0/8`, `::1`, or `localhost`.
pub(crate) fn is_loopback(url: &reqwest::Url) -> bool {
    if url
        .host_str()
        .is_some_and(|host| host.eq_ignore_ascii_case("localhost"))
    {
        return true;
    }

    literal(url).is_some_and(|ip| ip.is_loopback())
}

/// Whether a URL's host is an IP address rather than a name.
///
/// The URL parser has already normalised every spelling of an address —
/// `0x7f.1`, `2130706433` — into its canonical form, so parsing that form is
/// the whole test.
pub(crate) fn is_ip_literal(url: &reqwest::Url) -> bool {
    literal(url).is_some()
}

/// The address a URL's host is written as, if it is one.
fn literal(url: &reqwest::Url) -> Option<std::net::IpAddr> {
    let host = url.host_str()?;
    let bare = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);

    bare.parse().ok()
}
