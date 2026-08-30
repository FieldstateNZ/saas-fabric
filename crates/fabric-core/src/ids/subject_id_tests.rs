//! What a subject identifier accepts, and what it refuses.
//!
//! The refusals matter more than usual here, because the authorization service
//! refuses none of them. `acme/`, `/subject` and `acme//doubled` are all
//! ordinary identifiers to it — each a distinct subject that will never match
//! the one intended, so a malformed value would fail as a silently denied
//! request rather than as an error anybody sees.

use crate::{IdentifierError, SubjectId};

/// A Keycloak subject, which is what these normally are.
const SUB: &str = "cb606ddc-f148-4193-8875-a84ea6a85e6c";

#[test]
fn renders_as_realm_slash_subject() {
    let subject = SubjectId::from_verified("acme", SUB).expect("a valid subject");

    assert_eq!(subject.to_string(), format!("acme/{SUB}"));
    assert_eq!(subject.realm(), "acme");
    assert_eq!(subject.subject(), SUB);
}

#[test]
fn the_same_subject_in_two_realms_is_two_subjects() {
    // The whole reason the realm is part of the name: a provider's subject is
    // unique within its realm and nowhere else.
    let acme = SubjectId::from_verified("acme", SUB).expect("valid");
    let foo = SubjectId::from_verified("foo", SUB).expect("valid");

    assert_ne!(acme, foo);
    assert_ne!(acme.to_string(), foo.to_string());
}

#[test]
fn the_realm_takes_the_platforms_realm_rule() {
    // Uppercase is refused rather than folded, exactly as a realm name is
    // everywhere else, so `Acme` and `acme` cannot become the same subject at
    // one layer and different ones at another.
    assert!(matches!(
        SubjectId::from_verified("Acme", SUB),
        Err(IdentifierError::DisallowedCharacter { .. })
    ));
}

#[test]
fn an_empty_subject_is_refused() {
    // This is the `acme/` the authorization service would have accepted.
    assert_eq!(
        SubjectId::from_verified("acme", ""),
        Err(IdentifierError::Empty { kind: "subject" })
    );
}

#[test]
fn a_subject_containing_the_separator_is_refused() {
    // This is the `acme//doubled` case, and the one that would let a subject
    // forge its own realm qualification.
    assert!(matches!(
        SubjectId::from_verified("acme", "/doubled"),
        Err(IdentifierError::DisallowedCharacter { character: '/', .. })
    ));
}

#[test]
fn the_authorization_services_reserved_characters_are_refused() {
    for reserved in [':', '#', '*'] {
        let value = format!("sub{reserved}suffix");

        assert!(
            matches!(
                SubjectId::from_verified("acme", &value),
                Err(IdentifierError::DisallowedCharacter { .. })
            ),
            "{reserved:?} must be refused: it would make the identifier parse as something else"
        );
    }
}

#[test]
fn whitespace_is_refused() {
    assert!(matches!(
        SubjectId::from_verified("acme", "two words"),
        Err(IdentifierError::DisallowedCharacter { .. })
    ));
}

#[test]
fn an_unbounded_subject_is_refused() {
    let long = "x".repeat(256);

    assert!(matches!(
        SubjectId::from_verified("acme", &long),
        Err(IdentifierError::TooLong {
            max: 255,
            actual: 256,
            ..
        })
    ));
}

#[test]
fn a_subject_a_provider_might_actually_mint_is_accepted() {
    // Not every provider mints a UUID: an email-shaped subject and a numeric
    // one are both ordinary, and neither may be refused for looking unusual.
    for value in ["user@example.com", "104283719283746152837", "a.b-c_d"] {
        assert!(
            SubjectId::from_verified("acme", value).is_ok(),
            "{value} is a legitimate provider subject"
        );
    }
}
