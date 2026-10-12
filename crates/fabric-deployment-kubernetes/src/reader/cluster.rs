//! The three read-only API reads a sample makes, behind a seam for tests.

use async_trait::async_trait;

use crate::client::Client;
use crate::wire::{Deployment, Pod, ReplicaSet};

#[async_trait]
pub(crate) trait Cluster: Send + Sync {
    async fn deployment(&self, namespace: &str, name: &str) -> Result<Deployment, String>;
    async fn replica_sets(&self, namespace: &str, selector: &str) -> Result<Vec<ReplicaSet>, String>;
    async fn pods(&self, namespace: &str, selector: &str) -> Result<Vec<Pod>, String>;
}

#[async_trait]
impl Cluster for Client {
    async fn deployment(&self, namespace: &str, name: &str) -> Result<Deployment, String> {
        self.get(
            &format!("/apis/apps/v1/namespaces/{namespace}/deployments/{name}"),
            None,
        )
        .await
    }

    async fn replica_sets(&self, namespace: &str, selector: &str) -> Result<Vec<ReplicaSet>, String> {
        self.list(
            &format!("/apis/apps/v1/namespaces/{namespace}/replicasets"),
            selector,
        )
        .await
    }

    async fn pods(&self, namespace: &str, selector: &str) -> Result<Vec<Pod>, String> {
        self.list(&format!("/api/v1/namespaces/{namespace}/pods"), selector)
            .await
    }
}
