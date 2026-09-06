//! Recognising a host as an IP address literal, in every spelling curl, a
//! browser or a libc resolver still reads as one.
//!
//! Its own file because `std::net::IpAddr::from_str` is strict — it refuses
//! `127.1`, `2130706433` and `0x7f.0.0.1` — while `inet_aton`, and therefore
//! every one of those callers, reads all three as `127.0.0.1`. A classifier
//! that only asked the strict parser was answering a question nobody
//! downstream was actually asking.

use std::net::{IpAddr, Ipv4Addr};

/// Parses a host as an IP address literal.
///
/// Tries every form [`IpAddr`]'s own parser accepts, then the `inet_aton`
/// abbreviations it does not: dotted-decimal with fewer than four labels, a
/// single all-numeric value, and octal or `0x`-hexadecimal labels.
pub(super) fn parse(host: &str) -> Option<IpAddr> {
    if let Ok(address) = host.parse::<IpAddr>() {
        return Some(address);
    }

    parse_ipv4(host).map(IpAddr::V4)
}

/// Whether an address is the loopback address, following the IPv4-mapped
/// IPv6 spelling to the address it maps to.
pub(super) fn is_loopback(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => v6.is_loopback() || v6.to_ipv4_mapped().is_some_and(|mapped| mapped.is_loopback()),
    }
}

/// Parses 1 to 4 dot-separated labels the way `inet_aton` combines them: a
/// short form's last label absorbs whatever bytes a full four-label address
/// would have carried, which is what makes `127.1` mean `127.0.0.1`.
fn parse_ipv4(host: &str) -> Option<Ipv4Addr> {
    let labels: Vec<u32> = host.split('.').map(parse_c_integer).collect::<Option<_>>()?;

    let combined = match labels.as_slice() {
        [value] => *value,
        [first, second] if *first <= 0xFF && *second <= 0x00FF_FFFF => (*first << 24) | *second,
        [first, second, third] if *first <= 0xFF && *second <= 0xFF && *third <= 0xFFFF => {
            (*first << 24) | (*second << 16) | *third
        }
        [first, second, third, fourth]
            if *first <= 0xFF && *second <= 0xFF && *third <= 0xFF && *fourth <= 0xFF =>
        {
            (*first << 24) | (*second << 16) | (*third << 8) | *fourth
        }
        _ => return None,
    };

    Some(Ipv4Addr::from(combined))
}

/// Parses one label the way `inet_aton` parses it: decimal, `0`-prefixed
/// octal, or `0x`-prefixed hexadecimal.
///
/// Only the lower-cased `0x` is tested, and that is correct rather than a gap:
/// `super::super::kind::classify` lower-cases the host before any of this
/// runs, so `0X7F000001` has already become `0x7f000001` by the time it
/// arrives here.
fn parse_c_integer(label: &str) -> Option<u32> {
    if let Some(hex) = label.strip_prefix("0x") {
        if hex.is_empty() || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
            return None;
        }
        return u32::from_str_radix(hex, 16).ok();
    }

    if label.len() > 1 && label.starts_with('0') {
        if !label.chars().all(|character| ('0'..='7').contains(&character)) {
            return None;
        }
        return u32::from_str_radix(label, 8).ok();
    }

    if !label.is_empty() && label.chars().all(|character| character.is_ascii_digit()) {
        return label.parse().ok();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_forms_parse_through_the_strict_parser() {
        assert_eq!(parse("127.0.0.1"), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(parse("::1").is_some());
        assert!(parse("2001:db8::1").is_some());
    }

    #[test]
    fn abbreviated_dotted_decimal_combines_like_inet_aton() {
        assert_eq!(parse("127.1"), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert_eq!(parse("127.0.1"), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }

    #[test]
    fn a_single_decimal_number_is_a_32_bit_address() {
        assert_eq!(parse("2130706433"), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert_eq!(parse("134744072"), Some(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
    }

    #[test]
    fn hex_and_octal_labels_are_read() {
        assert_eq!(parse("0x7f000001"), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert_eq!(parse("0x7f.0.0.1"), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert_eq!(parse("0177.0.0.1"), Some(IpAddr::V4(Ipv4Addr::LOCALHOST)));
    }

    #[test]
    fn a_label_out_of_range_is_not_an_address() {
        assert_eq!(parse("256.0.0.1"), None);
        assert_eq!(parse("1.2.3.4.5"), None);
    }

    #[test]
    fn an_ordinary_hostname_is_not_an_address() {
        assert_eq!(parse("www.example.com"), None);
        assert_eq!(parse("0.example.com"), None);
    }

    #[test]
    fn the_ipv4_mapped_ipv6_spelling_of_loopback_is_loopback() {
        let address = parse("::ffff:127.0.0.1").expect("must parse");
        assert!(is_loopback(address));
    }

    #[test]
    fn a_public_address_is_not_loopback() {
        let address = parse("8.8.8.8").expect("must parse");
        assert!(!is_loopback(address));
    }
}
