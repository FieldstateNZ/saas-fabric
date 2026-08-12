//! Item 14: the priming-order guarantee that `build_runtime` documents —
//! DataSources load before tenant bindings, so a binding referencing a
//! DataSource that has not loaded yet never produces a spurious
//! `MissingDataSource` in the first moments after startup.

use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;

use crate::resource::{RegistryResource, ResourceSource};
use crate::testing::{data_source, tenant_binding};
use crate::{build_runtime, DataSource, RuntimeConfig, SourceError, TenantRuntimeBinding};

/// A source that records when it was asked to load, so the test can assert
/// on the order two independent sources were primed in.
struct OrderRecordingSource<T> {
    resources: Vec<T>,
    order: Arc<Mutex<Vec<&'static str>>>,
    label: &'static str,
}

#[async_trait]
impl<T: RegistryResource> ResourceSource<T> for OrderRecordingSource<T> {
    async fn load(&self) -> Result<Vec<T>, SourceError> {
        self.order
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(self.label);

        Ok(self.resources.clone())
    }

    fn describe(&self) -> String {
        self.label.to_owned()
    }
}

#[tokio::test]
async fn data_sources_prime_before_tenant_bindings() {
    let order = Arc::new(Mutex::new(Vec::new()));

    let data_source_source: Arc<OrderRecordingSource<DataSource>> = Arc::new(OrderRecordingSource {
        resources: vec![data_source("shared-01", 1)],
        order: Arc::clone(&order),
        label: "data_sources",
    });
    let tenant_source: Arc<OrderRecordingSource<TenantRuntimeBinding>> = Arc::new(OrderRecordingSource {
        resources: vec![tenant_binding("acme", 1, "shared-01")],
        order: Arc::clone(&order),
        label: "tenants",
    });

    let (_resolver, handles) = build_runtime(&RuntimeConfig::default(), tenant_source, data_source_source)
        .await
        .unwrap();

    handles.shutdown().await.unwrap();

    let order = order.lock().unwrap_or_else(PoisonError::into_inner);
    assert_eq!(
        order.as_slice(),
        ["data_sources", "tenants"],
        "a tenant binding referencing a DataSource must never be primed before that DataSource, \
         or it would resolve to a spurious MissingDataSource in the first moments after startup"
    );
}
