//! Precedence is the reason this type exists, so it is what these test.

use crate::{Channel, Version};

/// Pairs in ascending precedence. Half are from the `SemVer` specification's own
/// example; the rest are what this repository will actually publish.
const ASCENDING: [(&str, &str); 12] = [
    ("1.0.0-alpha", "1.0.0-alpha.1"),
    ("1.0.0-alpha.1", "1.0.0-alpha.beta"),
    ("1.0.0-alpha.beta", "1.0.0-beta"),
    ("1.0.0-beta", "1.0.0-beta.2"),
    ("1.0.0-beta.2", "1.0.0-beta.11"),
    ("1.0.0-beta.11", "1.0.0-rc.1"),
    ("1.0.0-rc.1", "1.0.0"),
    ("0.3.0-preview.9", "0.3.0-preview.10"),
    ("0.3.0-preview.20260831.9", "0.3.0-preview.20260831.42"),
    ("0.2.2", "0.3.0-preview.1"),
    ("0.3.0-preview.1", "0.3.0"),
    ("0.9.0", "0.10.0"),
];

fn version(text: &str) -> Version {
    Version::parse(text).unwrap_or_else(|| panic!("{text} should parse"))
}

#[test]
fn precedence_is_semver_and_not_string_order() {
    for (lower, higher) in ASCENDING {
        assert!(
            version(lower) < version(higher),
            "{lower} should precede {higher}"
        );
        assert!(version(higher) > version(lower), "{higher} should follow {lower}");
    }
}

#[test]
fn these_cases_would_be_wrong_under_string_order() {
    // Without this, the table above could be satisfied by comparing strings,
    // and a regression to `<` on `&str` would pass every test in this file.
    let disagreements = ASCENDING.iter().filter(|(lower, higher)| lower >= higher).count();

    assert!(
        disagreements >= 5,
        "only {disagreements} cases distinguish precedence from string order"
    );
}

#[test]
fn what_is_not_a_version_is_refused() {
    for text in [
        "latest",
        "1.0",
        "v1.0.0",
        "1.0.0+build",
        "01.0.0",
        "1.0.0-",
        "1.0.0.0",
        "",
        "1.0.0-pre..1",
        "1.0.0-pre_1",
    ] {
        assert!(Version::parse(text).is_none(), "{text} was accepted");
    }
}

#[test]
fn a_prerelease_part_is_what_makes_a_preview() {
    assert_eq!(version("0.3.0").channel(), Channel::Stable);
    assert_eq!(version("0.3.0-preview.1").channel(), Channel::Preview);
    assert_eq!(version("0.3.0-rc.1").channel(), Channel::Preview);
}

#[test]
fn a_series_is_the_version_core() {
    let series = version("0.3.0");

    assert!(version("0.3.0-preview.7").is_series(&series));
    assert!(version("0.3.0").is_series(&series));
    assert!(!version("0.3.1-preview.1").is_series(&series));
    assert!(!version("0.4.0").is_series(&series));
}

#[test]
fn build_metadata_is_a_chart_thing_and_never_an_image_thing() {
    // `+` is not legal in an OCI tag, so a version carrying one could never
    // name its own image. A chart is not an image and Helm permits it.
    assert!(Version::parse("1.2.3+build.7").is_none());
    assert!(Version::parse_chart("1.2.3+build.7").is_some());
}

#[test]
fn build_metadata_is_written_back_but_takes_no_part_in_precedence() {
    let plain = Version::parse_chart("1.2.3").expect("a version");
    let built = Version::parse_chart("1.2.3+build.7").expect("a version");

    // SemVer: build metadata is ignored when comparing.
    assert_eq!(plain.cmp(&built), std::cmp::Ordering::Equal);

    // And equality agrees with ordering, which is why `Eq` is written rather
    // than derived over the text. A `BTreeSet` that disagreed with `Ord` would
    // keep both or keep one depending on insertion order.
    assert_eq!(plain, built);

    // What gets written is still what was published.
    assert_eq!(built.as_str(), "1.2.3+build.7");
}

#[test]
fn empty_or_illegal_build_metadata_is_not_a_version() {
    for text in ["1.2.3+", "1.2.3+has spaces", "1.2.3+has/slash"] {
        assert!(Version::parse_chart(text).is_none(), "{text}");
    }
}

#[test]
fn build_metadata_is_dot_separated_identifiers_not_one_character_run() {
    // Validating `+foo.` as one run of `[0-9A-Za-z.-]` accepts a `.` in any
    // position, including a leading, trailing, or doubled one -- none of
    // which SemVer's §10 grammar (dot-separated, each identifier non-empty)
    // allows. Such a version could be selected as an upgrade and written
    // into Argo's `targetRevision`, so this has to be refused at parse time.
    for text in [
        "1.2.4+foo.",
        "1.2.4+.foo",
        "1.2.4+foo..bar",
        "1.2.4+",
        "1.2.4+foo_bar",
    ] {
        assert!(Version::parse_chart(text).is_none(), "{text} should be refused");
    }
}

#[test]
fn build_metadata_identifiers_may_carry_a_leading_zero() {
    // Unlike a numeric prerelease identifier, SemVer's §10 puts no
    // restriction on leading zeroes in build metadata -- it is never
    // compared, so there is nothing for a leading zero to make ambiguous.
    for text in [
        "1.2.4+foo.bar",
        "1.2.4+001",
        "1.2.4+0.0",
        "1.2.4+build-7",
        "1.2.4+20130313144700",
    ] {
        assert!(Version::parse_chart(text).is_some(), "{text} should parse");
    }
}

#[test]
fn a_prerelease_and_its_build_metadata_are_both_validated() {
    let version = Version::parse_chart("1.2.4-rc.1+foo.bar").expect("both parts are legal identifiers");

    assert_eq!(version.as_str(), "1.2.4-rc.1+foo.bar");
    assert_eq!(version.channel(), Channel::Preview);
}

#[test]
fn a_numeric_prerelease_identifier_may_not_carry_a_leading_zero() {
    for text in [
        "1.0.0-01",
        "1.0.0-alpha.01",
        "1.0.0-0.01",
        "1.0.0-00",
        // `parse_chart` runs the same prerelease grammar as `parse`, whether
        // or not the version also carries build metadata.
        "1.0.0-01+build",
    ] {
        assert!(Version::parse_chart(text).is_none(), "{text} should be refused");
    }
}

#[test]
fn a_numeric_prerelease_identifier_without_a_leading_zero_still_parses() {
    for text in [
        "1.0.0-0",
        "1.0.0-01a",
        "1.0.0-0a",
        "1.0.0-alpha.0",
        "1.0.0-10",
        "1.0.0-1.0.0",
        "0.3.0-preview.20260831.9",
    ] {
        assert!(Version::parse(text).is_some(), "{text} should parse");
        assert!(
            Version::parse_chart(text).is_some(),
            "{text} should parse as a chart version"
        );
    }
}

#[test]
fn a_numeric_prerelease_identifier_larger_than_a_u64_still_orders_by_number() {
    // `SemVer` puts no upper bound on a numeric prerelease identifier, so
    // this must parse and order correctly even past `u64::MAX`
    // (18446744073709551615). An earlier implementation compared numeric
    // identifiers by parsing them as `u64` and fell back to string order on
    // overflow — which would have put this identifier below "9999" instead
    // of above it.
    assert!(Version::parse("1.0.0-18446744073709551616").is_some());
    assert!(Version::parse_chart("1.0.0-18446744073709551616").is_some());

    assert!(version("1.0.0-9999") < version("1.0.0-18446744073709551615"));
    assert!(version("1.0.0-18446744073709551615") < version("1.0.0-18446744073709551616"));

    // Numeric identifiers still rank below alphanumeric ones, however large.
    assert!(version("1.0.0-18446744073709551616") < version("1.0.0-a"));
}
