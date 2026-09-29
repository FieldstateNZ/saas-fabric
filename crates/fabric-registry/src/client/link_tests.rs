//! Reading a `Link` header's next target.

use super::link::next_target;

#[test]
fn the_next_link_is_found_among_others() {
    assert_eq!(
        next_target(r#"</v2/a/tags/list?last=b&n=5>; rel="next""#),
        Some("/v2/a/tags/list?last=b&n=5")
    );
    assert_eq!(
        next_target(r#"</first>; rel="prev", <https://ghcr.io/v2/a/tags/list?last=c>; rel=next"#),
        Some("https://ghcr.io/v2/a/tags/list?last=c")
    );
}

#[test]
fn a_header_with_no_next_link_names_none() {
    assert_eq!(next_target(r#"</first>; rel="prev""#), None);
    assert_eq!(next_target("garbage"), None);
    assert_eq!(next_target(r#"</next>; title="rel=next""#), None);
}
