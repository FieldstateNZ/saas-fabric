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

/// The highest revision an id has ever been published at, and whether the
/// published data-sources document still holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Published {
    pub(crate) revision: BindingRevision,
    pub(crate) current: bool,
}

/// Plans a declaration against what an environment currently holds.
///
/// # The revision rule
///
/// An id that is already held keeps its revision when every other field is
/// unchanged -- `Plan::Unchanged`, and no write at all -- provided the
/// publisher would take it: at or above the revision it is published at
/// now, or above the one it was last published at before it was removed.
/// Anything else is written one above the higher of the held revision and
/// the published one: so a new id starts at 1, and an id removed and
/// declared again, or held lower after a Git revert, comes back above every
/// revision a reader may still hold rather than restarting below it (O3,
/// D01-1). The revision an incoming declaration carries is never trusted;
/// it is always recomputed here, which is why the comparison below sets it
/// aside before deciding whether anything actually changed.
pub(crate) fn plan(
    held: &[DataSourceDeclaration],
    incoming: &DataSourceDeclaration,
    published: Option<Published>,
) -> Plan {
    let floor = published.map_or(BindingRevision::ZERO, |published| published.revision);
    let keeps = match published {
        Some(Published {
            revision,
            current: false,
        }) => revision.next(),
        _ => floor,
    };
    let Some(existing) = held.iter().find(|declared| declared.id == incoming.id) else {
        let mut declarations = held.to_vec();
        declarations.push(DataSourceDeclaration {
            revision: floor.next(),
            ..incoming.clone()
        });
        declarations.sort_by(|left, right| left.id.cmp(&right.id));

        return Plan::Write(declarations);
    };

    let merged = DataSourceDeclaration {
        revision: existing.revision,
        ..incoming.clone()
    };

    if &merged == existing && existing.revision >= keeps {
        return Plan::Unchanged;
    }

    let mut declarations: Vec<DataSourceDeclaration> = held
        .iter()
        .filter(|declared| declared.id != incoming.id)
        .cloned()
        .collect();
    declarations.push(DataSourceDeclaration {
        revision: existing.revision.max(floor).next(),
        ..merged
    });
    declarations.sort_by(|left, right| left.id.cmp(&right.id));

    Plan::Write(declarations)
}
