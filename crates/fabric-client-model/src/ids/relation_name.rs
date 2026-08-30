//! The name of a relation a subject can hold on a resource.

slug_newtype!(
    /// What a subject *is* to a resource — `viewer`, `editor`, `owner`.
    ///
    /// A relation is a noun, not a permission. `viewer` says how somebody is
    /// related to a resource; which operations that permits is stated
    /// separately, in the same declaration, because the two answers change
    /// independently: widening what an editor may do should not require
    /// inventing a new word for an editor.
    ///
    /// Takes the permissive identifier rule rather than the DNS one:
    /// `billing_admin` is a reasonable relation and reads better than
    /// `billing-admin` to the platform engineers who write these documents.
    RelationName,
    "relation name",
    fabric_core::naming::parse_identifier
);
