//! Deciding what a declaration should write, from what is held.

#[cfg(test)]
#[path = "plan_tests.rs"]
mod plan_tests;

use fabric_core::BindingRevision;

use crate::data_sources::declaration::DataSourceDeclaration;

/// What `DataSources::declare` should do, decided from the declarations an
/// environment already holds and the one it was asked to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Plan {
    /// The incoming declaration matches what is already held, once its
    /// revision is set aside. Nothing is written.
    Unchanged,

    /// The complete list to write, already carrying the computed revision
    /// and sorted by id.
    Write(Vec<DataSourceDeclaration>),
}

/// Plans a declaration against what an environment currently holds.
///
/// # The revision rule
///
/// A new id starts at revision 1. An id that is already held keeps its
/// revision when every other field is unchanged -- `Plan::Unchanged`, and
/// no write at all -- or moves to `held + 1` otherwise. The revision an
/// incoming declaration carries is never trusted; it is always recomputed
/// here, which is why the comparison below sets it aside before deciding
/// whether anything actually changed.
pub(crate) fn plan(held: &[DataSourceDeclaration], incoming: &DataSourceDeclaration) -> Plan {
    let Some(existing) = held.iter().find(|declared| declared.id == incoming.id) else {
        let mut declarations = held.to_vec();
        declarations.push(DataSourceDeclaration {
            revision: BindingRevision::new(1),
            ..incoming.clone()
        });
        declarations.sort_by(|left, right| left.id.cmp(&right.id));

        return Plan::Write(declarations);
    };

    let merged = DataSourceDeclaration {
        revision: existing.revision,
        ..incoming.clone()
    };

    if &merged == existing {
        return Plan::Unchanged;
    }

    let mut declarations: Vec<DataSourceDeclaration> = held
        .iter()
        .filter(|declared| declared.id != incoming.id)
        .cloned()
        .collect();
    declarations.push(DataSourceDeclaration {
        revision: existing.revision.next(),
        ..merged
    });
    declarations.sort_by(|left, right| left.id.cmp(&right.id));

    Plan::Write(declarations)
}
