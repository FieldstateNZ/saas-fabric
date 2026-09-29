//! Where a URL points: its origin, and whether that is loopback.

/// Whether two URLs share an origin: scheme, host and port, a port left
/// implicit being the scheme's own.
pub(crate) fn same_origin(one: &reqwest::Url, other: &reqwest::Url) -> bool {
    one.scheme() == other.scheme()
        && one.host_str() == other.host_str()
        && one.port_or_known_default() == other.port_or_known_default()
}

/// Whether a URL's host is loopback: `127.0.0.0/8`, `::1`, or `localhost`.
pub(crate) fn is_loopback(url: &reqwest::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };

    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    let bare = host
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(host);

    bare.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}
