//! Given what is held, and what came in: unchanged, or the whole list to write.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, PlacementClassDocument, PoolSettingsDocument,
};

use super::{plan, Plan};
use crate::DataSourceDeclaration;

fn declaration(id: &str, revision: u64) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
        revision: BindingRevision::new(revision),
        connector: ConnectorId::try_new("postgres-nz").expect("a valid connector id"),
        connection: ConnectionSelectorDocument::Named {
            name: ConnectionName::try_new("shared").expect("a valid connection name"),
        },
        placement: PlacementClassDocument::Dedicated,
        residency: DataResidencyDocument {
            region: "nz".to_owned(),
            jurisdiction: None,
        },
        pool: PoolSettingsDocument::default(),
        capabilities: DataSourceCapabilitiesDocument::default(),
        discriminator: None,
        labels: BTreeMap::new(),
    }
}

#[test]
fn a_new_id_starts_at_revision_one() {
    let incoming = declaration("b", 0);

    let Plan::Write(declarations) = plan(&[], &incoming) else {
        panic!("a first declaration must write");
    };

    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].revision, BindingRevision::new(1));
}

#[test]
fn an_unchanged_declaration_writes_nothing() {
    let held = vec![declaration("a", 3)];
    let incoming = declaration("a", 0);

    assert_eq!(plan(&held, &incoming), Plan::Unchanged);
}

#[test]
fn a_changed_field_moves_the_revision_forward_by_one() {
    let held = vec![declaration("a", 3)];
    let mut incoming = declaration("a", 0);
    incoming.residency.region = "au".to_owned();

    let Plan::Write(declarations) = plan(&held, &incoming) else {
        panic!("a changed field must write");
    };

    assert_eq!(declarations.len(), 1);
    assert_eq!(declarations[0].revision, BindingRevision::new(4));
    assert_eq!(declarations[0].residency.region, "au");
}

#[test]
fn an_incoming_revision_is_never_trusted() {
    let held = vec![declaration("a", 3)];
    let incoming = declaration("a", 999);

    assert_eq!(plan(&held, &incoming), Plan::Unchanged);
}

#[test]
fn the_list_stays_sorted_by_id_after_a_new_declaration() {
    let held = vec![declaration("b", 1)];
    let incoming = declaration("a", 0);

    let Plan::Write(declarations) = plan(&held, &incoming) else {
        panic!("a new id must write");
    };

    let ids: Vec<&str> = declarations.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "b"]);
}

#[test]
fn the_list_stays_sorted_by_id_after_a_correction() {
    let held = vec![declaration("a", 1), declaration("c", 1)];
    let mut incoming = declaration("a", 0);
    incoming.residency.region = "au".to_owned();

    let Plan::Write(declarations) = plan(&held, &incoming) else {
        panic!("a changed field must write");
    };

    let ids: Vec<&str> = declarations.iter().map(|item| item.id.as_str()).collect();
    assert_eq!(ids, vec!["a", "c"]);
}
