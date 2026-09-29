//! A repository's first component: its registry host, and a port.
use crate::errors::{invalid, ContractError};

/// Docker Hub's canonical host, the only one its repositories are written
/// with.
const DOCKER_HUB: &str = "docker.io";

/// Other names Docker Hub answers to, each a second spelling of
/// [`DOCKER_HUB`].
const DOCKER_HUB_ALIASES: [&str; 3] = [
    "index.docker.io",
    "registry-1.docker.io",
    "registry.hub.docker.com",
];

/// The longest host name, from the DNS specification.
const MAX_HOST: usize = 253;

/// Checks the host component, given the path after it for Docker Hub's rule.
pub(super) fn check(component: &str, path: &str) -> Result<(), ContractError> {
    if !(component.contains('.') || component.contains(':') || component == "localhost") {
        return Err(invalid(format!(
            "A repository starts with its registry host, which contains a dot or a port, or is localhost: {component} is not one"
        )));
    }
    let (host, port) = match component.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (component, None),
    };
    if host.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(invalid(format!(
            "A registry host is written in lower case: {host}"
        )));
    }
    if host.is_empty()
        || host.len() > MAX_HOST
        || host
            .split('.')
            .any(|label| fabric_core::naming::parse_dns_label("registry host", label).is_err())
    {
        return Err(invalid(format!("A registry host must be a DNS name: {host}")));
    }
    if ends_in_a_number(host) {
        return Err(invalid(format!(
            "A registry host is a name, not an IP address: {host}"
        )));
    }
    if let Some(port) = port {
        check_port(port)?;
    }
    if DOCKER_HUB_ALIASES.contains(&host) {
        return Err(invalid(format!(
            "Docker Hub repositories are written {DOCKER_HUB}/..., not {host}/..."
        )));
    }
    if host == DOCKER_HUB && !path.contains('/') {
        return Err(invalid(format!(
            "A Docker Hub repository of one path segment is written {DOCKER_HUB}/library/{path}"
        )));
    }
    Ok(())
}

/// Whether a URL parser would read `host` as an IPv4 address: WHATWG's
/// "ends in a number" -- its last label all decimal digits, or `0x` and hex
/// digits. Upper case was refused already, so `0X` cannot reach here.
///
/// # Why the last label, and not every label
///
/// `127.1`, `0x7f000001` and `0x7f.1` are all 127.0.0.1 to a URL parser,
/// and `registry.123` is no name at all to one. v1's rules may only relax, so
/// the refusal is written as wide as a parser's reading now.
fn ends_in_a_number(host: &str) -> bool {
    let last = host.rsplit('.').next().unwrap_or(host);
    let decimal = !last.is_empty() && last.bytes().all(|byte| byte.is_ascii_digit());
    let hex = last
        .strip_prefix("0x")
        .is_some_and(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()));
    decimal || hex
}

/// A port is 1 to 65535, written without a leading zero, and never 443,
/// which HTTPS already implies.
fn check_port(port: &str) -> Result<(), ContractError> {
    let number = port.parse::<u16>().ok().filter(|number| {
        *number != 0 && port.bytes().all(|byte| byte.is_ascii_digit()) && !port.starts_with('0')
    });
    match number {
        None => Err(invalid(format!(
            "A registry port must be a number from 1 to 65535: {port}"
        ))),
        Some(443) => Err(invalid(
            "A registry host is written without port 443, which HTTPS implies",
        )),
        Some(_) => Ok(()),
    }
}
