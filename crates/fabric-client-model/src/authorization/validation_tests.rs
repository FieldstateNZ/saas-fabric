//! What an authorization configuration accepts, and what it refuses.
//!
//! The refusals are the interesting half. Each one is a document an operator
//! could plausibly write and which would reconcile into something nobody meant
//! — a resource nothing can reach, a relation granting nothing, or a
//! declaration that says two different things about the same name.

use fabric_core::{LogicalResourceName, OperationKind};

use crate::{AuthorizationConfiguration, DesiredStateError, Relation, RelationName, ResourceAuthorization};

/// A resource name, for a test that is not about resource names.
fn resource(name: &str) -> LogicalResourceName {
    LogicalResourceName::try_new(name).expect("a valid resource name")
}

/// A relation permitting the operations named.
fn relation(name: &str, permits: &[OperationKind]) -> Relation {
    Relation {
        name: RelationName::try_new(name).expect("a valid relation name"),
        permits: permits.to_vec(),
    }
}

/// One resource with the relations given.
fn declaring(name: &str, relations: Vec<Relation>) -> AuthorizationConfiguration {
    AuthorizationConfiguration {
        resources: vec![ResourceAuthorization {
            resource: resource(name),
            relations,
        }],
    }
}

#[test]
fn a_configuration_declaring_nothing_is_valid() {
    // Every document written before this section existed parses to this, and
    // refusing it would make the model unable to read its own past output.
    let configuration = AuthorizationConfiguration::default();

    assert!(configuration.validate().is_ok());
    assert!(configuration.is_empty());
}

#[test]
fn relations_may_permit_overlapping_operations() {
    // An editor and an owner both reading is the normal case, not a collision.
    let configuration = declaring(
        "customers",
        vec![
            relation("viewer", &[OperationKind::Read, OperationKind::List]),
            relation(
                "editor",
                &[OperationKind::Read, OperationKind::List, OperationKind::Update],
            ),
        ],
    );

    assert!(configuration.validate().is_ok());
}

#[test]
fn the_same_resource_cannot_be_declared_twice() {
    let configuration = AuthorizationConfiguration {
        resources: vec![
            ResourceAuthorization {
                resource: resource("customers"),
                relations: vec![relation("viewer", &[OperationKind::Read])],
            },
            ResourceAuthorization {
                resource: resource("customers"),
                relations: vec![relation("owner", &[OperationKind::Delete])],
            },
        ],
    };

    assert_eq!(
        configuration.validate(),
        Err(DesiredStateError::Duplicate {
            field: "spec.authorization.resources",
            value: "customers".to_owned(),
        })
    );
}

#[test]
fn a_resource_with_no_relations_is_refused() {
    let configuration = declaring("customers", vec![]);

    let Err(DesiredStateError::InvalidField { field, detail }) = configuration.validate() else {
        panic!("expected a resource with no relations to be refused");
    };

    assert_eq!(field, "spec.authorization.resources[].relations");
    assert!(
        detail.contains("customers"),
        "the message names the resource: {detail}"
    );
}

#[test]
fn the_same_relation_cannot_be_declared_twice_on_one_resource() {
    let configuration = declaring(
        "customers",
        vec![
            relation("editor", &[OperationKind::Read]),
            relation("editor", &[OperationKind::Delete]),
        ],
    );

    assert_eq!(
        configuration.validate(),
        Err(DesiredStateError::Duplicate {
            field: "spec.authorization.resources[].relations",
            value: "editor".to_owned(),
        })
    );
}

#[test]
fn the_same_relation_on_two_resources_is_fine() {
    // `viewer` means something different on each, and both are needed.
    let configuration = AuthorizationConfiguration {
        resources: vec![
            ResourceAuthorization {
                resource: resource("customers"),
                relations: vec![relation("viewer", &[OperationKind::Read])],
            },
            ResourceAuthorization {
                resource: resource("orders"),
                relations: vec![relation("viewer", &[OperationKind::Read])],
            },
        ],
    };

    assert!(configuration.validate().is_ok());
}

#[test]
fn a_relation_permitting_nothing_is_refused() {
    let configuration = declaring("customers", vec![relation("ghost", &[])]);

    let Err(DesiredStateError::InvalidField { field, detail }) = configuration.validate() else {
        panic!("expected a relation permitting nothing to be refused");
    };

    assert_eq!(field, "spec.authorization.resources[].relations[].permits");
    assert!(
        detail.contains("ghost"),
        "the message names the relation: {detail}"
    );
}

#[test]
fn an_operation_cannot_be_permitted_twice_by_one_relation() {
    let configuration = declaring(
        "customers",
        vec![relation("editor", &[OperationKind::Read, OperationKind::Read])],
    );

    assert_eq!(
        configuration.validate(),
        Err(DesiredStateError::Duplicate {
            field: "spec.authorization.resources[].relations[].permits",
            value: "read".to_owned(),
        })
    );
}

#[test]
fn a_relation_reports_what_it_permits() {
    let editor = relation("editor", &[OperationKind::Read, OperationKind::Update]);

    assert!(editor.permits(OperationKind::Update));
    assert!(!editor.permits(OperationKind::Delete));
}
