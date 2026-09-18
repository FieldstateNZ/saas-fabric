//! A held document a break-glass edit could have made incoherent.

use std::collections::BTreeMap;

use fabric_core::{BindingRevision, DataSourceId};
use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, PlacementClassDocument, PoolSettingsDocument,
};

use super::check_held;
use crate::{DataSourceDeclaration, PlatformError};

fn declaration(id: &str) -> DataSourceDeclaration {
    DataSourceDeclaration {
        id: DataSourceId::try_new(id).expect("a valid data source id"),
        revision: BindingRevision::new(1),
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
fn a_coherent_document_is_accepted() {
    assert!(check_held(&[declaration("a"), declaration("b")]).is_ok());
}

#[test]
fn two_entries_with_one_id_are_refused() {
    let failure = check_held(&[declaration("a"), declaration("a")]).unwrap_err();

    let PlatformError::InvalidHeldDataSources { detail } = failure else {
        panic!("expected InvalidHeldDataSources, got {failure:?}");
    };
    assert!(detail.contains('a'), "{detail}");
    assert!(
        !detail.contains('/'),
        "no path in an operator-facing message: {detail}"
    );
}

#[test]
fn an_entry_that_no_longer_validates_is_refused() {
    let mut invalid = declaration("a");
    invalid.placement = PlacementClassDocument::Shared;
    invalid.discriminator = None;

    let failure = check_held(&[invalid]).unwrap_err();

    let PlatformError::InvalidHeldDataSources { detail } = failure else {
        panic!("expected InvalidHeldDataSources, got {failure:?}");
    };
    assert!(detail.contains('a'), "{detail}");
    assert!(detail.contains("discriminator"), "{detail}");
    assert!(
        !detail.contains('/'),
        "no path in an operator-facing message: {detail}"
    );
}
