//! Tests for [`RoleName`](super::RoleName)'s rule.

use super::RoleName;

#[test]
fn accepts_the_platform_required_roles() {
    for role in crate::required_roles::REQUIRED_ROLES {
        assert!(
            RoleName::try_new(role).is_ok(),
            "{role} must be a legal role name"
        );
    }
}

#[test]
fn a_doubled_interior_space_is_refused() {
    // The failure this closes: a role that renders identically to a required
    // one, compares unequal against Keycloak, and is therefore re-created on
    // every reconciliation pass.
    assert!(RoleName::try_new("Client  Realm User").is_err());
}

#[test]
fn leading_and_trailing_whitespace_is_refused_rather_than_trimmed() {
    // Trimming would make " Client Realm User" and "Client Realm User" the
    // same role at one layer and different roles at another.
    assert!(RoleName::try_new(" Client Realm User").is_err());
    assert!(RoleName::try_new("Client Realm User ").is_err());
}

#[test]
fn a_slash_is_refused_because_the_name_reaches_an_admin_api_path() {
    assert!(RoleName::try_new("Client/Realm/User").is_err());
}

#[test]
fn deserialising_runs_the_same_validation_as_the_constructor() {
    let error = serde_norway::from_str::<RoleName>("\" leading space\"").unwrap_err();

    assert!(error.to_string().contains("start and end"));
}

#[test]
fn round_trips_through_yaml() {
    let role = RoleName::try_new("Client Realm Administrator").unwrap();
    let encoded = serde_norway::to_string(&role).unwrap();

    assert_eq!(serde_norway::from_str::<RoleName>(&encoded).unwrap(), role);
}
