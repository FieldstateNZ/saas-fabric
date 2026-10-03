//! Which IP addresses are public.
//!
//! # Refused, and why each
//!
//! Loopback and unspecified are this host. Link-local covers the cloud
//! metadata endpoints (`169.254.169.254`, `fe80::/10`). Private, shared
//! (`100.64.0.0/10`, carrier NAT) and unique-local are someone's internal
//! network. Multicast, reserved, documentation and benchmarking ranges are
//! never a registry. An IPv6 address carrying an IPv4 one — mapped
//! (`::ffff:a.b.c.d`), compatible (`::a.b.c.d`), the well-known NAT64 prefix
//! (`64:ff9b::/96`) or 6to4 (`2002:wwxx:yyzz::/48`) — is judged as the IPv4
//! address it reaches, so `::ffff:127.0.0.1` is loopback.
//!
//! The local-use NAT64 prefix (`64:ff9b:1::/48`, RFC 8215) and Teredo
//! (`2001::/32`) are refused outright: a local NAT64 may embed its IPv4
//! address anywhere in the prefix a network chose, and Teredo obscures it, so
//! neither can be judged by the address it reaches — and no registry is
//! served from either.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Whether `ip` is an address a registered registry may be read at.
pub(crate) fn is_public(ip: IpAddr) -> bool {
    match normalised(ip) {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

/// Whether `ip` is loopback, however it is written.
pub(crate) fn is_loopback(ip: IpAddr) -> bool {
    normalised(ip).is_loopback()
}

/// `ip`, or the IPv4 address an IPv6 one carries.
pub(crate) fn normalised(ip: IpAddr) -> IpAddr {
    let IpAddr::V6(v6) = ip else {
        return ip;
    };
    if let Some(v4) = v6.to_ipv4_mapped() {
        return IpAddr::V4(v4);
    }
    let segments = v6.segments();
    let prefix = segments.get(..6).unwrap_or_default();
    let [.., w, x, y, z] = v6.octets();
    let compatible = prefix == [0; 6] && !v6.is_loopback() && !v6.is_unspecified();
    let nat64 = prefix == [0x64, 0xff9b, 0, 0, 0, 0];
    if compatible || nat64 {
        return IpAddr::V4(Ipv4Addr::new(w, x, y, z));
    }
    if let [0x2002, high, low, ..] = segments {
        return IpAddr::V4(Ipv4Addr::from((u32::from(high) << 16) | u32::from(low)));
    }
    ip
}

/// An IPv4 address outside every refused range.
fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    let refused = a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        // Multicast, reserved and broadcast: 224.0.0.0 and everything above.
        || a >= 224;
    !refused
}

/// An IPv6 address outside every refused range.
fn public_v6(ip: Ipv6Addr) -> bool {
    let [a, b, c, d, ..] = ip.segments();
    let refused = ip.is_unspecified()
        || ip.is_loopback()
        || a & 0xff00 == 0xff00 // multicast
        || a & 0xfe00 == 0xfc00 // unique-local
        || a & 0xffc0 == 0xfe80 // link-local
        || a & 0xffc0 == 0xfec0 // site-local, deprecated and still private
        || (a == 0x2001 && b == 0x0db8) // documentation
        || (a == 0x3fff && b < 0x1000) // documentation, 3fff::/20
        || (a == 0x2001 && b == 0x0002 && c == 0) // benchmarking
        || (a == 0x0064 && b == 0xff9b && c == 0x0001) // local-use NAT64
        || (a == 0x2001 && b == 0) // Teredo
        || (a == 0x0100 && b == 0 && c == 0 && d == 0); // discard-only
    !refused
}
