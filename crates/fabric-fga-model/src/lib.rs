//! The model writer: a client's declared authorization, as the authorization
//! model a tenant's store is pinned to (ADR 0013, ADR 0027 §2a).
//!
//! ```text
//! spec.authorization (fabric-client-model)      customers: viewer permits [read, list]
//!         │
//!         ▼  authorization_model()
//! user
//! customers               viewer        can_read  can_update  can_delete
//! customers_collection    viewer        can_list  can_create
//!         │
//!         ▼  not built: the front's control-plane surface writes it (ADR 0016)
//! a tenant's store, pinned by model id
//! ```
//!
//! # Records and collections are different types
//!
//! `read`, `update` and `delete` are asked about a record, `customers:42`.
//! `list` and `create` name no record, so they are asked about the resource's
//! collection, `customers_collection:customers`. Each type defines only its
//! own operations' computed relations, so a record allow cannot answer a
//! collection question, even inside the authorization service. Nothing links
//! the two: holding `viewer` on the collection grants nothing on any record,
//! and the reverse. A `can_list` allow authorizes the collection response; it
//! does not filter rows.
//!
//! # One spelling, shared with the runtime
//!
//! The collection type and the `can_<operation>` names come from `fabric-core`
//! ([`CollectionObject`](fabric_core::CollectionObject),
//! [`OperationKind::computed_relation`](fabric_core::OperationKind::computed_relation)),
//! which `fabric-fga-check` reads too. The control plane writes what the
//! runtime asks, and neither plane depends on the other.
//!
//! # Nothing calls this yet
//!
//! It is a pure translation with no transport. Writing the model through the
//! front and pinning it per issuer is ADR 0027's Owed item 3.

mod model;
mod translate;

pub use model::AuthorizationModel;
pub use translate::authorization_model;
