//! Finding the parts of a message that must not be shown.

use super::CREDENTIAL_PREFIXES;

/// Replaces credential-shaped runs with a marker.
pub(super) fn redact(text: &str) -> String {
    // A PEM block is redacted whole. Nothing after `-----BEGIN` in an error
    // message is worth showing, and the key material is what follows.
    if let Some(begin) = text.find("-----BEGIN") {
        let mut kept = text.get(..begin).unwrap_or_default().to_owned();
        kept.push_str("[redacted]");
        return kept;
    }

    let mut words: Vec<String> = Vec::new();
    let mut previous_named_a_credential = false;

    for word in text.split_whitespace() {
        // `Bearer <value>` and `token=<value>` put the secret in the *next*
        // word, so what came before decides this one.
        words.push(if previous_named_a_credential {
            "[redacted]".to_owned()
        } else {
            redact_word(word)
        });

        previous_named_a_credential = introduces_a_credential(word);
    }

    words.join(" ")
}

/// Redacts one whitespace-separated word.
fn redact_word(word: &str) -> String {
    if CREDENTIAL_PREFIXES.iter().any(|prefix| word.contains(prefix)) {
        return "[redacted]".to_owned();
    }

    // A URL's path says which registry and which repository, which is worth
    // reading. Its query can carry a signature or a token, and is not.
    if let Some(query) = word.find('?') {
        if word.starts_with("http://") || word.starts_with("https://") {
            let mut kept = word.get(..query).unwrap_or_default().to_owned();
            kept.push_str("?[redacted]");
            return kept;
        }
    }

    // `token=value` is one word, so the lookback below cannot see it. The name
    // is kept because knowing *which* credential was refused is useful and the
    // value never is.
    if let Some((name, _)) = word.split_once('=') {
        if introduces_a_credential(name) {
            return format!("{name}=[redacted]");
        }
    }

    word.to_owned()
}

/// Whether a word follows one that names a credential.
///
/// `Authorization: Bearer <value>` and `token=<value>` put the secret in the
/// *next* word, so the word before decides.
fn introduces_a_credential(word: &str) -> bool {
    let lowered = word.to_lowercase();

    [
        "bearer",
        "token",
        "password",
        "secret",
        "authorization:",
        "authorization",
    ]
    .iter()
    .any(|name| lowered.trim_end_matches(&[':', '='][..]) == *name)
}
