//! The realm roles every client must have.

/// The realm roles the platform requires in every client realm.
///
/// These are part of SaaS Fabric's contract with a client rather than a
/// Keycloak convention: the platform's own authorisation model assumes a realm
/// distinguishes an administrator of the client from an ordinary user of it,
/// and every client-facing surface built on top of that assumption breaks if
/// one of them is missing.
///
/// They are `&'static str` rather than parsed [`RoleName`](crate::RoleName)
/// values because a `RoleName` owns a `String` and cannot be built in a
/// constant. Parsing them at every use site instead would need a fallible
/// constructor in code that has nothing sensible to do with the failure —
/// `role_name_tests` asserts that both are legal names, which is the same
/// guarantee without the ceremony.
pub const REQUIRED_ROLES: [&str; 2] = ["Client Realm Administrator", "Client Realm User"];

/// Returns the first required role that is absent from `roles`.
///
/// Order follows [`REQUIRED_ROLES`], so an operator who removed both is told
/// about the administrator role first and gets a stable message rather than
/// one that depends on how their list happened to be ordered.
#[must_use]
pub fn first_missing(roles: &[crate::RoleName]) -> Option<&'static str> {
    REQUIRED_ROLES
        .into_iter()
        .find(|required| !roles.iter().any(|role| role.as_str() == *required))
}
