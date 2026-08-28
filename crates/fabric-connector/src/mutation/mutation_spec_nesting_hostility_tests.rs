//! Adversarial test: a caller filter that is not a flat predicate but a deep
//! tree mixing every connective. `for_target` must reach the same guarantee
//! — the tenant predicate conjoined — without accidentally only handling a
//! shallow, single-clause filter.

use serde_json::Value;

use crate::testing::{collection, discriminator_target, equals, field, tenant_predicate};
use crate::{Filter, MutationSpec, Row};

#[test]
fn deeply_nested_caller_filters_still_get_the_tenant_predicate_conjoined() {
    // And( Or( Not(Compare), And(IsNull, In) ), Compare ) — several levels,
    // mixing every connective.
    let nested = Filter::And {
        clauses: vec![
            Filter::Or {
                clauses: vec![
                    Filter::Not {
                        clause: Box::new(equals("status", "closed")),
                    },
                    Filter::And {
                        clauses: vec![
                            Filter::IsNull {
                                field: field("archived_at"),
                            },
                            Filter::In {
                                field: field("region"),
                                values: vec![Value::String("au".into()), Value::String("nz".into())],
                            },
                        ],
                    },
                ],
            },
            equals("owner", "alice"),
        ],
    };

    let spec = MutationSpec::Update {
        collection: collection(),
        filter: Some(nested.clone()),
        changes: Row::new(),
    };

    let MutationSpec::Update { filter, .. } = spec.for_target(&discriminator_target()) else {
        panic!("expected an update");
    };

    // `Filter::and` flattens a top-level `And`, so the tenant predicate joins
    // the existing top-level clauses rather than wrapping the whole tree —
    // but the nested Or/Not/And subtree must come through completely intact.
    let Filter::And { clauses } = &nested else {
        unreachable!("constructed as And above");
    };
    let Some(Filter::And {
        clauses: executed_clauses,
    }) = filter
    else {
        panic!("expected a top-level conjunction");
    };
    for original in clauses {
        assert!(
            executed_clauses.contains(original),
            "nested clause {original:?} did not survive untouched"
        );
    }
    assert!(executed_clauses.contains(&tenant_predicate()));
}
