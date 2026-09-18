//! The Kubernetes adapter for runtime publication: three `ConfigMap`s in
//! `platform-system`, written over plain HTTPS with the pod's own identity
//! (ADR 0018, "The production owner"; ADR 0023 part 4).
//!
//! # What this crate is, and is not
//!
//! It implements [`fabric_runtime_publication::RuntimePublication`] and
//! nothing else. Which documents change, at which revision, and which
//! offers are refused is decided by
//! [`fabric_runtime_publication::plan_publication`], shared with the
//! filesystem adapter, so this crate cannot drift from it: what is left here
//! is reading three objects, and writing the ones the plan says to write, in
//! the order the plan declares them.
//!
//! # Why no `kube` crate
//!
//! The workspace bans Kubernetes client crates everywhere
//! (`scripts/check_architecture.py`). `fabric-deployment-kubernetes` showed
//! the shape that needs none: `reqwest` against `https://kubernetes.default.svc`,
//! the projected service-account token re-read on every request so rotation
//! is honoured, the cluster CA from the mounted root, no redirects, and a
//! bounded response. This crate takes exactly that shape, for three objects
//! of one kind.

mod client;
mod config;
mod errors;
mod held;
mod object;
mod publish;
mod wire;

pub use config::PublicationTarget;
pub use publish::KubernetesRuntimePublication;

#[cfg(test)]
mod testing;
