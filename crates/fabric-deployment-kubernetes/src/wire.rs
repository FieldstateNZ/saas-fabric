//! The small read-only subset of Kubernetes resources used as evidence.
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Metadata {
    pub uid: String,
    #[serde(default)]
    pub generation: u64,
    pub resource_version: String,
    pub deletion_timestamp: Option<String>,
    #[serde(default)]
    pub owner_references: Vec<Owner>,
}
#[derive(Deserialize)]
pub(crate) struct Owner {
    pub uid: String,
    pub kind: String,
    #[serde(default)]
    pub controller: bool,
}
#[derive(Deserialize)]
pub(crate) struct Deployment {
    pub metadata: Metadata,
    pub spec: DeploymentSpec,
    #[serde(default)]
    pub status: DeploymentStatus,
}
#[derive(Deserialize)]
pub(crate) struct DeploymentSpec {
    pub replicas: u32,
    pub selector: Selector,
    pub template: Template,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Selector {
    #[serde(default)]
    pub match_labels: BTreeMap<String, String>,
    #[serde(default)]
    pub match_expressions: Vec<serde_json::Value>,
}
#[derive(Deserialize)]
pub(crate) struct Template {
    pub spec: PodSpec,
}
#[derive(Deserialize)]
pub(crate) struct PodSpec {
    pub containers: Vec<Container>,
}
#[derive(Deserialize)]
pub(crate) struct Container {
    pub name: String,
    pub image: String,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeploymentStatus {
    #[serde(default)]
    pub observed_generation: u64,
    #[serde(default)]
    pub replicas: u32,
    #[serde(default)]
    pub updated_replicas: u32,
    #[serde(default)]
    pub available_replicas: u32,
    #[serde(default)]
    pub ready_replicas: u32,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}
#[derive(Deserialize)]
pub(crate) struct Condition {
    pub r#type: String,
    pub status: String,
}
#[derive(Deserialize)]
pub(crate) struct ReplicaSet {
    pub metadata: Metadata,
}
#[derive(Deserialize)]
pub(crate) struct Pod {
    pub metadata: Metadata,
    pub spec: PodSpec,
    #[serde(default)]
    pub status: PodStatus,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PodStatus {
    #[serde(default)]
    pub container_statuses: Vec<ContainerStatus>,
    #[serde(default)]
    pub conditions: Vec<Condition>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContainerStatus {
    pub name: String,
    pub ready: bool,
    #[serde(rename = "imageID", default)]
    pub image_id: String,
    #[serde(default)]
    pub state: BTreeMap<String, serde_json::Value>,
}
#[derive(Deserialize)]
pub(crate) struct List<T> {
    pub metadata: ListMetadata,
    pub items: Vec<T>,
}
#[derive(Deserialize)]
pub(crate) struct ListMetadata {
    #[serde(rename = "continue", default)]
    pub continuation: String,
}
