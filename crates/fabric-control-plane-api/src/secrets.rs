//! Turning a configured secret reference into a value.

/// The prefix applied to every derived variable name.
///
/// The same convention the runtime host uses, and deliberately the same: an
/// operator who has learned how one SaaS Fabric process is given a secret
/// should not have to learn a second scheme for the other.
pub const PREFIX: &str = "FABRIC_SECRET_";

/// Reads the value a configured reference names.
///
/// # What this is, and what it is not
///
/// It reads the process environment. That covers every delivery mechanism the
/// platform actually uses — the External Secrets Operator and the OpenBao
/// agent both project values into a pod's environment — so it is not a toy,
/// but it is also not a client for a secret store.
///
/// The application's side of the contract is only that a value called
/// something arrives; **how** it arrives belongs to `saas-fabric-platform`
/// (§20, §21). That separation is why the configuration holds a reference
/// rather than a value, and why this function is the whole of the
/// implementation.
///
/// # Errors
///
/// Returns a message naming the *variable*, never a value. A missing secret is
/// a startup failure: a control plane that cannot authenticate to Git or
/// Keycloak can do nothing useful, and discovering that at startup beats
/// discovering it on an operator's first write.
pub fn resolve(reference: &str) -> Result<String, String> {
    let name = variable_name(reference);

    std::env::var(&name).map_err(|_| format!("the secret {reference} is not set; expected {name}"))
}

/// Converts a reference into an environment variable name.
///
/// Every character outside `A-Z` and `0-9` becomes an underscore, so the
/// mapping is total and predictable. It is not injective — `a/b` and `a-b`
/// collide — which is called out here rather than left to be discovered.
fn variable_name(reference: &str) -> String {
    let sanitised: String = reference
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();

    format!("{PREFIX}{sanitised}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_a_variable_name_from_a_reference() {
        assert_eq!(
            variable_name("keycloak/saas-fabric"),
            "FABRIC_SECRET_KEYCLOAK_SAAS_FABRIC"
        );
    }

    #[test]
    fn a_missing_secret_names_the_variable_and_not_a_value() {
        let error = resolve("git/nonexistent").unwrap_err();

        assert!(error.contains("FABRIC_SECRET_GIT_NONEXISTENT"));
    }
}
