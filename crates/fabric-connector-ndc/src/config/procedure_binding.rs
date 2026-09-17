//! One procedure and the argument names it expects.

use std::collections::BTreeMap;

use crate::config::PayloadShape;

/// One procedure and the argument names it expects.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcedureBinding {
    /// The procedure's name in the connector's schema.
    pub procedure: String,

    /// The argument carrying the payload — rows for an insert, field changes
    /// for an update.
    ///
    /// Must differ from [`Self::filter_argument`], and startup validation
    /// rejects a mapping where it does not: both are placed in the same
    /// argument map, so a shared name means the predicate lands on top of the
    /// payload and the write silently changes nothing.
    #[serde(default)]
    pub payload_argument: Option<String>,

    /// The argument carrying the predicate, for updates and deletes.
    ///
    /// A mapping for an update or delete that omits this is rejected at
    /// startup: without somewhere to put the predicate, the tenant scoping
    /// added by
    /// [`MutationSpec::for_target`](fabric_connector::MutationSpec::for_target)
    /// would be silently dropped, and the write would reach every tenant's rows.
    ///
    /// Nothing in the specification fixes this name; a real `ndc-postgres`
    /// calls it `pre_check` on the delete and update procedures it generates,
    /// and `post_check` on the insert procedure's own permission predicate
    /// (observed against `ndc-postgres` v3.1.0, issue #62) — never `filter`,
    /// which the shipped example wrongly used until that observation. Set this
    /// to whatever name the target connector's own `/schema` declares; startup
    /// checks the name and its kind against that schema either way.
    #[serde(default)]
    pub filter_argument: Option<String>,

    /// Physical field name (as it appears in the neutral [`Filter`](fabric_connector::Filter))
    /// → the procedure's own argument that carries that field's key value.
    ///
    /// # Why a key travels separately from the predicate
    ///
    /// A real `ndc-postgres` generates update and delete procedures keyed by
    /// primary key: `delete_articles_by_id_and_tenant_key(key_id, key_tenant_key,
    /// pre_check)`. The predicate the platform built still goes out in full
    /// under [`Self::filter_argument`] — this map only says which of its
    /// equalities also have to be repeated as a named key argument, because
    /// the procedure has nowhere else to read its key from.
    ///
    /// When the discriminator column is one of these fields, its value now
    /// reaches the connector **twice**: once as a key argument, once inside
    /// the predicate. That is defence in depth, not redundancy to trim — the
    /// two are checked and built independently, so a mistake in one does not
    /// silently disable the other.
    ///
    /// Empty by default, which is the only legal value on an insert: an
    /// insert has no key to scope by, and a mapping declaring one here is
    /// refused at config validation. Translation never guesses a key value —
    /// it is read only from an equality in the predicate the platform already
    /// built, never from the caller's payload, and a key field with no such
    /// equality is refused rather than defaulted.
    #[serde(default)]
    pub key_arguments: BTreeMap<String, String>,

    /// How [`Self::payload_argument`]'s value is shaped, for an update.
    ///
    /// Defaults to [`PayloadShape::Values`] — `{col: value}`, what every
    /// mapping sent before `ndc-postgres`'s keyed update procedures existed.
    /// See [`PayloadShape`] for why a second shape exists at all, and
    /// `config` validation for why only an update may choose it.
    #[serde(default)]
    pub payload_shape: PayloadShape,
}

impl ProcedureBinding {
    /// The payload and predicate argument names, when the mapping declares
    /// both.
    ///
    /// `None` when either is absent — a mapping naming only one of them has
    /// nothing to collide with, so validation has nothing to compare.
    pub(super) fn argument_names(&self) -> Option<(&String, &String)> {
        self.payload_argument.as_ref().zip(self.filter_argument.as_ref())
    }
}
