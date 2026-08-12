//! The file source, and the one behaviour that matters most about it.

use std::path::PathBuf;

use fabric_core::BindingRevision;

use crate::resource::sources::JsonFileSource;
use crate::resource::ResourceSource;
use crate::{DataSource, SourceError, TenantRuntimeBinding};

/// Writes a file into a fresh directory under the process temp dir.
///
/// Deliberately not a temp-file crate: the need is three lines, and a
/// dependency for it is not worth the supply chain.
fn write_temp(name: &str, contents: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!("fabric-source-{name}"));
    std::fs::create_dir_all(&directory).unwrap();

    let path = directory.join("resources.json");
    std::fs::write(&path, contents).unwrap();
    path
}

#[tokio::test]
async fn reads_tenant_bindings_from_a_json_array() {
    let path = write_temp(
        "tenants",
        r#"[
            {
                "tenant": "acme",
                "revision": 42,
                "data": {
                    "primary": {
                        "data_source": "sql-au-east-03",
                        "isolation": {"kind": "database"}
                    }
                }
            }
        ]"#,
    );

    let bindings: Vec<TenantRuntimeBinding> = JsonFileSource::new(&path).load().await.unwrap();

    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings.first().unwrap().revision, BindingRevision::new(42));
}

#[tokio::test]
async fn reads_data_sources_from_their_own_file() {
    // The two resources are reconciled independently, which means separate
    // files: changing a pool size does not rewrite tenant state.
    let path = write_temp(
        "data-sources",
        r#"[
            {
                "id": "sql-au-east-03",
                "revision": 4,
                "connector": "postgres-au-east",
                "connection": {"kind": "named", "name": "acme-prod"},
                "placement": "dedicated",
                "residency": {"region": "au-east"}
            }
        ]"#,
    );

    let sources: Vec<DataSource> = JsonFileSource::new(&path).load().await.unwrap();

    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources.first().unwrap().pool.max_connections,
        20,
        "defaults applied"
    );
}

#[tokio::test]
async fn an_empty_array_is_a_legitimate_empty_set() {
    let path = write_temp("empty", "[]");

    let loaded: Vec<TenantRuntimeBinding> = JsonFileSource::new(&path).load().await.unwrap();

    assert!(loaded.is_empty());
}

#[tokio::test]
async fn a_missing_file_is_an_error_not_an_empty_set() {
    // The important one: an unreadable mount must not deprovision everything.
    let source: JsonFileSource<TenantRuntimeBinding> = JsonFileSource::new("/nonexistent/fabric/x.json");

    assert!(matches!(
        source.load().await.unwrap_err(),
        SourceError::Unreadable { .. }
    ));
}

#[tokio::test]
async fn malformed_json_is_an_error_not_an_empty_set() {
    let path = write_temp("malformed", "{ not json");
    let source: JsonFileSource<TenantRuntimeBinding> = JsonFileSource::new(&path);

    assert!(matches!(
        source.load().await.unwrap_err(),
        SourceError::Malformed { .. }
    ));
}

#[tokio::test]
async fn an_invalid_tenant_id_is_rejected_at_the_boundary() {
    let path = write_temp("bad-tenant", r#"[{"tenant": "Acme Corp", "revision": 1}]"#);
    let source: JsonFileSource<TenantRuntimeBinding> = JsonFileSource::new(&path);

    assert!(matches!(
        source.load().await.unwrap_err(),
        SourceError::Malformed { .. }
    ));
}

#[tokio::test]
async fn a_tenant_binding_carrying_connector_configuration_is_rejected() {
    // Physical configuration belongs to the DataSource. A binding that tries to
    // carry it is a reconciler written against the old model, and silently
    // ignoring the field would leave the tenant pointing somewhere unintended.
    let path = write_temp(
        "leaky-binding",
        r#"[{"tenant":"acme","revision":1,"data":{"primary":{"data_source":"x","isolation":{"kind":"database"},"connector":"postgres"}}}]"#,
    );
    let source: JsonFileSource<TenantRuntimeBinding> = JsonFileSource::new(&path);

    assert!(matches!(
        source.load().await.unwrap_err(),
        SourceError::Malformed { .. }
    ));
}
