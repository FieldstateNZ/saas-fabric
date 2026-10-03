//! A listing is never silently truncated: a page bound run past, a later
//! page that is not there, or one that is not a listing, is an error — never
//! the pages read so far returned as if they were all of them.
//!
//! Every page here is a reply a test fixed for a path. The fake answers the
//! first fixed reply whose path is a prefix of the request's, so a later
//! page is fixed under a query of its own, and before the page that links to
//! it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use fabric_platform_management::{Attached, Registry, RegistryError};
use fabric_registry::OciRegistry;
use serde_json::{json, Value};
use support::{FakeRegistry, HOST};

const RUNTIME: &str = "ghcr.io/fieldstatenz/saas-fabric";
const REFERRERS: &str = "/v2/fieldstatenz/saas-fabric/referrers/";
const TAGS: &str = "/v2/fieldstatenz/saas-fabric/tags/list";
const INDEX: &str = "application/vnd.oci.image.index.v1+json";
const MANIFEST: &str = "application/vnd.oci.image.manifest.v1+json";
const DOCUMENT: &str = r#"{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{}}"#;
const OTHER_DOCUMENT: &str = r#"{"apiVersion":"fabric.fieldstate.nz/v1","kind":"Component","spec":{"x":1}}"#;

fn registry(fake: &FakeRegistry) -> OciRegistry {
    OciRegistry::plain_http_to_loopback(&fake.base_url, HOST, 5).unwrap()
}

/// A registry with the image `0.3.0` published, and its digest.
async fn published() -> (FakeRegistry, String) {
    let fake = FakeRegistry::start().await;
    fake.publish(RUNTIME, "0.3.0", "5320432");
    let subject = fake.digest_for(RUNTIME, "0.3.0");
    (fake, subject)
}

/// A component descriptor for `document`, stored so a candidate fetch finds
/// it: its digest, and the entry a referrers page lists it under.
fn stored(fake: &FakeRegistry, subject: &str, document: &str) -> (String, Value) {
    let manifest = fake.descriptor_manifest(RUNTIME, subject, document).to_string();
    let digest = fake.put_manifest(RUNTIME, &manifest);
    let entry = json!({
        "mediaType": MANIFEST,
        "digest": digest.as_str(),
        "size": manifest.len(),
        "artifactType": fabric_component::ARTIFACT_TYPE
    });
    (digest, entry)
}

/// One referrers page: an OCI index of `entries`.
fn page(entries: &[Value]) -> String {
    json!({ "schemaVersion": 2, "mediaType": INDEX, "manifests": entries }).to_string()
}

/// A `Link` naming the next page at `path`, on the registry's own origin.
fn next(path: &str) -> String {
    format!("<{path}>; rel=\"next\"")
}

#[tokio::test]
async fn a_descriptor_on_a_later_referrers_page_is_not_lost() {
    let (fake, subject) = published().await;
    let (first, first_entry) = stored(&fake, &subject, DOCUMENT);
    let (second, second_entry) = stored(&fake, &subject, OTHER_DOCUMENT);
    let second_page = format!("{REFERRERS}{subject}?page=2");
    let link = next(&second_page);
    fake.answer(
        "GET",
        &second_page,
        200,
        &[("Content-Type", INDEX)],
        page(&[second_entry]),
    );
    fake.answer(
        "GET",
        REFERRERS,
        200,
        &[("Content-Type", INDEX), ("Link", link.as_str())],
        page(&[first_entry]),
    );

    let found = registry(&fake)
        .component_descriptor(RUNTIME, &subject)
        .await
        .unwrap();

    // Both, as `Several`: the first page alone would have been `One`.
    let Attached::Several { digests } = found else {
        panic!("one per page, both found: {found:?}");
    };
    let mut both = vec![first, second];
    both.sort();
    assert_eq!(digests, both);
    assert_eq!(fake.count("GET", REFERRERS), 2, "{:?}", fake.paths());
}

#[tokio::test]
async fn a_later_referrers_page_that_is_not_there_is_an_error_not_the_first_page_alone() {
    let (fake, subject) = published().await;
    let (_, first_entry) = stored(&fake, &subject, DOCUMENT);
    let second_page = format!("{REFERRERS}{subject}?page=2");
    let link = next(&second_page);
    fake.answer("GET", &second_page, 404, &[], "{}");
    fake.answer(
        "GET",
        REFERRERS,
        200,
        &[("Content-Type", INDEX), ("Link", link.as_str())],
        page(&[first_entry]),
    );

    let failure = registry(&fake).component_descriptor(RUNTIME, &subject).await;

    // A later `404` is not the answer a first-page `404` is: it is a listing
    // cut short, refused rather than retried.
    assert!(
        matches!(failure, Err(RegistryError::Refused { .. })),
        "never `One` from the first page alone: {failure:?}"
    );
    assert_eq!(fake.count("GET", REFERRERS), 2, "{:?}", fake.paths());
}

#[tokio::test]
async fn a_later_referrers_page_that_is_not_an_index_is_an_error_not_the_first_page_alone() {
    let (fake, subject) = published().await;
    let (_, first_entry) = stored(&fake, &subject, DOCUMENT);
    let second_page = format!("{REFERRERS}{subject}?page=2");
    let link = next(&second_page);
    // `200`, and JSON of no particular type: on the first page that would
    // mean the API is not served; on a later one it is a listing cut short.
    fake.answer("GET", &second_page, 200, &[], "{}");
    fake.answer(
        "GET",
        REFERRERS,
        200,
        &[("Content-Type", INDEX), ("Link", link.as_str())],
        page(&[first_entry]),
    );

    let failure = registry(&fake).component_descriptor(RUNTIME, &subject).await;

    assert!(
        matches!(failure, Err(RegistryError::Unavailable { .. })),
        "never `One` from the first page alone: {failure:?}"
    );
    assert_eq!(fake.count("GET", REFERRERS), 2, "{:?}", fake.paths());
}

#[tokio::test]
async fn referrers_paged_past_ten_pages_is_an_error_after_exactly_ten_requests() {
    let (fake, subject) = published().await;
    let (_, entry) = stored(&fake, &subject, DOCUMENT);
    let link = next(&format!("{REFERRERS}{subject}?again"));
    // One reply for every page, each naming a next: a listing with no end.
    fake.answer(
        "GET",
        REFERRERS,
        200,
        &[("Content-Type", INDEX), ("Link", link.as_str())],
        page(&[entry]),
    );

    let failure = registry(&fake).component_descriptor(RUNTIME, &subject).await;

    let Err(RegistryError::Unavailable { detail }) = failure else {
        panic!("expected Unavailable, got {failure:?}");
    };
    assert!(detail.contains("paged past 10 pages"), "{detail}");
    assert_eq!(fake.count("GET", REFERRERS), 10, "{:?}", fake.paths());
}

#[tokio::test]
async fn tags_paged_past_fifty_pages_is_an_error_after_exactly_fifty_requests() {
    let fake = FakeRegistry::start().await;
    let link = next(&format!("{TAGS}?last=again"));
    fake.answer(
        "GET",
        TAGS,
        200,
        &[("Link", link.as_str())],
        json!({ "tags": ["0.3.0"] }).to_string(),
    );

    let failure = registry(&fake).tags(RUNTIME).await;

    // Fifty pages of one tag were read; none of them is returned.
    let Err(RegistryError::Unavailable { detail }) = failure else {
        panic!("expected Unavailable, got {failure:?}");
    };
    assert!(detail.contains("paged past 50 pages"), "{detail}");
    assert_eq!(fake.count("GET", TAGS), 50, "{:?}", fake.paths().len());
}
