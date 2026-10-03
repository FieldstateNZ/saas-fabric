//! A component descriptor frozen inside another document.
use super::descriptor_tests::example;
use super::ComponentDescriptor;

fn descriptor() -> ComponentDescriptor {
    ComponentDescriptor::from_json(example().as_bytes()).unwrap()
}

fn yaml() -> String {
    serde_norway::to_string(&descriptor()).unwrap()
}

fn read(text: &str) -> Result<ComponentDescriptor, String> {
    serde_norway::from_str(text).map_err(|error| error.to_string())
}

#[test]
fn a_copy_is_its_whole_envelope_and_reads_back_equal() {
    let text = yaml();

    assert!(
        text.starts_with("apiVersion: fabric.fieldstate.nz/v1\nkind: Component\nspec:\n  name: reports\n"),
        "{text}"
    );
    assert_eq!(read(&text).unwrap(), descriptor());
}

#[test]
fn as_json_a_copy_is_the_canonical_bytes() {
    assert_eq!(serde_json::to_vec(&descriptor()).unwrap(), descriptor().to_json());
    assert_eq!(
        serde_json::from_slice::<ComponentDescriptor>(&descriptor().to_json()).unwrap(),
        descriptor()
    );
}

#[test]
fn a_copy_of_a_later_version_is_refused_by_its_version() {
    let later = yaml().replacen("fabric.fieldstate.nz/v1", "fabric.fieldstate.nz/v2", 1);
    // A field v1 does not know: the version is named before the spec is read.
    let later = later.replacen("  name: reports\n", "  name: reports\n  modules: []\n", 1);

    let error = read(&later).unwrap_err();

    assert!(
        error.contains("version v2 is not one this build reads"),
        "{error}"
    );
}

#[test]
fn a_copy_that_breaks_a_rule_is_refused() {
    let spoofed = yaml().replacen("title: Reports", "title: \"Rep\\u202Eorts\"", 1);
    let unknown = yaml().replacen("  name: reports\n", "  name: reports\n  modules: []\n", 1);
    let other_registry = yaml().replacen(
        "registry.example.com/acme/reports-web",
        "ghcr.io/acme/reports-web",
        1,
    );

    for (text, expected) in [
        (
            spoofed,
            "Component title must not contain Unicode format characters",
        ),
        (unknown, "modules"),
        (other_registry, "one registry"),
    ] {
        let error = read(&text).unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn a_copy_that_is_not_a_component_descriptor_is_refused_as_such() {
    let wrong = yaml().replacen("kind: Component", "kind: Catalogue", 1);
    let bare = "spec: {}\n";
    let no_spec = "apiVersion: fabric.fieldstate.nz/v1\nkind: Component\n";
    let extra = format!("{}extra: true\n", yaml());

    assert!(read(&wrong)
        .unwrap_err()
        .contains("found fabric.fieldstate.nz/v1/Catalogue"));
    assert!(read(bare).unwrap_err().contains("no apiVersion or kind at all"));
    assert!(read(no_spec).unwrap_err().contains("missing field `spec`"));
    assert!(read(&extra).unwrap_err().contains("extra"));
}

#[test]
fn serde_norway_refuses_a_duplicate_key_at_every_depth() {
    let text = yaml();
    let cases = [
        text.replacen("kind: Component\n", "kind: Component\nkind: Component\n", 1),
        text.replacen("  title: Reports\n", "  title: Reports\n  title: Other\n", 1),
        text.replacen("    web:\n", "    api:\n", 1),
        text.replacen("    label: Team\n", "    label: Team\n    label: Team\n", 1),
    ];
    for case in cases {
        assert_ne!(case, text);
        let error = read(&case).unwrap_err();
        assert!(error.contains("duplicate"), "{error}");
        let as_value = serde_norway::from_str::<serde_norway::Value>(&case).map(drop);
        assert!(as_value.is_err(), "a Value refuses it too: {case}");
    }
}
