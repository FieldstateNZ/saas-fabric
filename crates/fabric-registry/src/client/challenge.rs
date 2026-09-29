//! Reading a `WWW-Authenticate` header: which schemes a registry asks for,
//! and with which parameters (RFC 9110 section 11.6.1).
//!
//! # Why a parser, and not a split on commas
//!
//! A header may carry several challenges — `Basic realm="x", Bearer
//! realm="y"` — and a parameter's value may be a quoted string holding
//! commas, `=` and escaped quotes. Splitting on commas would read a realm
//! out of an `error_description`. This reads the grammar: a scheme, then
//! `name=value` parameters, each value a token or a quoted string, until a
//! token not followed by `=` begins the next challenge. Scheme and parameter
//! names are case-insensitive, and held lower-cased.
//!
//! A header that does not parse offers nothing: guessing what a malformed
//! challenge meant is how a realm nobody named gets a credential.

mod cursor;

use std::collections::BTreeMap;

use cursor::Cursor;

/// Which scheme a challenge asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Scheme {
    /// A token from a realm.
    Bearer,

    /// A username and password, sent to the registry itself.
    Basic,

    /// Anything else, which this adapter never answers.
    Other(String),
}

/// One challenge: a scheme and its parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Challenge {
    /// The scheme.
    pub(super) scheme: Scheme,

    /// Its parameters, by lower-cased name.
    pub(super) parameters: BTreeMap<String, String>,
}

impl Challenge {
    /// A parameter's value, by lower-case name.
    pub(super) fn parameter(&self, name: &str) -> Option<&str> {
        self.parameters.get(name).map(String::as_str)
    }
}

/// Every challenge in every `WWW-Authenticate` header of a response, a
/// header that is not text or does not parse contributing none.
pub(super) fn challenges(headers: &reqwest::header::HeaderMap) -> Vec<Challenge> {
    headers
        .get_all(reqwest::header::WWW_AUTHENTICATE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(parse)
        .flatten()
        .collect()
}

/// The challenges in one header value, or `None` if it does not parse.
pub(super) fn parse(value: &str) -> Option<Vec<Challenge>> {
    let mut cursor = Cursor::new(value);
    let mut found = Vec::new();

    loop {
        cursor.skip(b", \t");
        if cursor.peek().is_none() {
            return Some(found);
        }
        let scheme = match cursor.token()?.to_ascii_lowercase().as_str() {
            "bearer" => Scheme::Bearer,
            "basic" => Scheme::Basic,
            other => Scheme::Other(other.to_owned()),
        };
        let mut parameters = BTreeMap::new();
        while let Some(name) = cursor.parameter_name() {
            cursor.skip(b" \t");
            let value = if cursor.peek() == Some(b'"') {
                cursor.quoted()?
            } else {
                cursor.token()?.to_owned()
            };
            parameters.insert(name, value);
        }
        found.push(Challenge { scheme, parameters });
    }
}
