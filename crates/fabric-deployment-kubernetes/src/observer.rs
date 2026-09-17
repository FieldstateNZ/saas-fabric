//! Bounded live reads; failed observations never reuse an earlier success.
use crate::{
    client::Client,
    evaluate::evaluate,
    wire::{Deployment, Pod, ReplicaSet},
    WorkloadTarget,
};
use async_trait::async_trait;
use fabric_core::Clock;
use fabric_platform_management::{
    DeploymentHealth as Health, DeploymentObservation, DeploymentObserver, WorkloadObservation,
};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

/// A read-only observer authenticated with this pod's existing service account.
pub struct KubernetesObserver {
    pub(crate) client: Client,
    environment: String,
    targets: BTreeMap<String, Vec<WorkloadTarget>>,
    clock: Arc<dyn Clock>,
}
impl KubernetesObserver {
    /// Builds the adapter from an explicit deployment-owned workload allow-list.
    ///
    /// # Errors
    /// Refuses invalid targets or a missing cluster trust root. Credentials are
    /// read at request time so projected service-account token rotation works.
    pub fn in_cluster(
        environment: &str,
        targets: BTreeMap<String, Vec<WorkloadTarget>>,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, String> {
        for workloads in targets.values() {
            if workloads.is_empty() || workloads.len() > 16 {
                return Err("observation requires between one and sixteen workloads per component".into());
            }
            for target in workloads {
                target.validate()?;
            }
        }
        let root = "/var/run/secrets/kubernetes.io/serviceaccount";
        let pem = std::fs::read(format!("{root}/ca.crt"))
            .map_err(|_| "observation requires the cluster CA certificate")?;
        let ca = reqwest::Certificate::from_pem(&pem)
            .map_err(|_| "observation cluster CA certificate is invalid")?;
        let http = reqwest::Client::builder()
            .add_root_certificate(ca)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| "could not build the deployment observation client")?;
        Ok(Self {
            client: Client {
                http,
                base: "https://kubernetes.default.svc".into(),
                token_file: format!("{root}/token").into(),
            },
            environment: environment.into(),
            targets,
            clock,
        })
    }
    async fn workload(&self, target: &WorkloadTarget) -> Result<WorkloadObservation, String> {
        let path = format!(
            "/apis/apps/v1/namespaces/{}/deployments/{}",
            target.namespace, target.deployment
        );
        let before: Deployment = self.client.get(&path, None).await?;
        if before.spec.selector.match_labels.is_empty() || !before.spec.selector.match_expressions.is_empty()
        {
            return Err("Deployment observation requires a nonempty equality label selector.".into());
        }
        let selector = before
            .spec
            .selector
            .match_labels
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");
        let sets: Vec<ReplicaSet> = self
            .client
            .list(
                &format!("/apis/apps/v1/namespaces/{}/replicasets", target.namespace),
                &selector,
            )
            .await?;
        let pods: Vec<Pod> = self
            .client
            .list(
                &format!("/api/v1/namespaces/{}/pods", target.namespace),
                &selector,
            )
            .await?;
        let after: Deployment = self.client.get(&path, None).await?;
        if before.metadata.resource_version != after.metadata.resource_version
            || before.metadata.uid != after.metadata.uid
        {
            return Err("The deployment changed during observation; refresh to read it again.".into());
        }
        Ok(evaluate(target, &after, &sets, &pods))
    }
    async fn workloads(&self, targets: &[WorkloadTarget]) -> Vec<WorkloadObservation> {
        let mut rows = Vec::new();
        for target in targets {
            rows.push(
                self.workload(target)
                    .await
                    .unwrap_or_else(|detail| WorkloadObservation {
                        name: target.deployment.clone(),
                        health: Health::Unavailable,
                        versions: vec![],
                        desired_replicas: None,
                        ready_replicas: 0,
                        detail: Some(detail),
                    }),
            );
        }
        rows
    }
}
#[async_trait]
impl DeploymentObserver for KubernetesObserver {
    async fn observe(&self, environment: &str, component: &str) -> Option<DeploymentObservation> {
        if environment != self.environment {
            return None;
        }
        let targets = self.targets.get(component)?;
        let result = tokio::time::timeout(Duration::from_secs(5), self.workloads(targets)).await;
        let (workloads, detail) = match result {
            Ok(rows) => (rows, None),
            Err(_) => (
                vec![],
                Some("Deployment observation timed out; no current evidence is available.".into()),
            ),
        };
        Some(super::summary::summarize(
            workloads,
            self.clock.now_unix_seconds(),
            detail,
        ))
    }
}
