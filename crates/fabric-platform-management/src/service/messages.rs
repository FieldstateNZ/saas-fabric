//! What a commit that moves a component says about the release it wrote.

use crate::Release;

/// The message for advancing `component` in `environment` to `release`.
///
/// A release says what it can prove about itself, and no more. Images name
/// the commit they were built from; a chart has no commit to name, so the
/// message does not pretend to one; a described release names the commit
/// and the component descriptor's digest as well, because that digest is
/// recorded nowhere else — desired state holds the images, and the evidence
/// that made them one release unit is this line (ADR 0026 section 9).
pub(super) fn advance(environment: &str, component: &str, release: &Release) -> String {
    match release {
        Release::Unit(unit) => format!(
            "Advance {environment} to {component} {}\n\nBuilt from {}.",
            unit.version, unit.source_revision
        ),
        Release::Chart { version, .. } => {
            format!("Advance {environment} to {component} chart {version}")
        }
        Release::Described { unit, descriptor, .. } => format!(
            "Advance {environment} to {component} {}\n\nBuilt from {}.\nComponent descriptor {descriptor}.",
            unit.version, unit.source_revision
        ),
    }
}

/// The message for rolling `component` in `environment` back to `release`.
///
/// The version for every kind, and for a described release the commit it
/// was built from and the component descriptor's digest too, for the reason
/// [`advance`] gives: a rollback re-applies the rule as an advance does, and
/// its commit is where the evidence is kept.
pub(super) fn roll_back(environment: &str, component: &str, release: &Release) -> String {
    let headline = format!(
        "Roll {component} in {environment} back to {}",
        release.version().as_str()
    );

    match release {
        Release::Unit(_) | Release::Chart { .. } => headline,
        Release::Described { unit, descriptor, .. } => format!(
            "{headline}\n\nBuilt from {}.\nComponent descriptor {descriptor}.",
            unit.source_revision
        ),
    }
}
