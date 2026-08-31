//! Creating the blobs, trees and commits a change is made of.

use reqwest::Method;

use crate::host::wire::{Commit, Created, NewBlob, NewCommit, NewTree, TreeEntry};
use crate::host::PlatformGitRepository;
use crate::{CommitRevision, PlatformGitError};

/// A non-executable file. This adapter writes desired state, never a program.
const FILE_MODE: &str = "100644";

impl PlatformGitRepository {
    /// Creates a blob and returns its hash.
    ///
    /// Content-addressed, so creating the same text twice returns the same
    /// hash and costs nothing. That is why the retry loop builds blobs once
    /// and rebuilds only the tree and commit.
    pub(crate) async fn create_blob(&self, text: &str) -> Result<String, PlatformGitError> {
        let body = serde_json::to_value(NewBlob {
            content: text,
            encoding: "utf-8",
        })
        .map_err(|error| PlatformGitError::Unavailable {
            detail: format!("a blob could not be encoded: {error}"),
        })?;

        let created: Created = self
            .json(
                "creating a blob",
                Method::POST,
                self.url("git/blobs"),
                Some(body),
                None,
            )
            .await?;

        Ok(created.sha)
    }

    /// The tree a commit points at.
    pub(crate) async fn tree_of(&self, commit: &CommitRevision) -> Result<String, PlatformGitError> {
        let url = self.url(&format!("git/commits/{}", commit.as_str()));

        let commit: Commit = self
            .json("reading a commit", Method::GET, url, None, None)
            .await?;

        Ok(commit.tree.sha)
    }

    /// Creates a tree layered on `base_tree`, changing only the paths given.
    pub(crate) async fn create_tree(
        &self,
        base_tree: &str,
        entries: &[(String, String)],
    ) -> Result<String, PlatformGitError> {
        let tree = entries
            .iter()
            .map(|(path, blob)| TreeEntry {
                path,
                mode: FILE_MODE,
                kind: "blob",
                sha: blob,
            })
            .collect();

        let body = serde_json::to_value(NewTree { base_tree, tree }).map_err(|error| {
            PlatformGitError::Unavailable {
                detail: format!("a tree could not be encoded: {error}"),
            }
        })?;

        let created: Created = self
            .json(
                "creating a tree",
                Method::POST,
                self.url("git/trees"),
                Some(body),
                None,
            )
            .await?;

        Ok(created.sha)
    }

    /// Creates a commit with exactly one parent.
    pub(crate) async fn create_commit(
        &self,
        message: &str,
        tree: &str,
        parent: &CommitRevision,
    ) -> Result<CommitRevision, PlatformGitError> {
        let body = serde_json::to_value(NewCommit {
            message,
            tree,
            parents: vec![parent.as_str()],
        })
        .map_err(|error| PlatformGitError::Unavailable {
            detail: format!("a commit could not be encoded: {error}"),
        })?;

        let created: Created = self
            .json(
                "creating a commit",
                Method::POST,
                self.url("git/commits"),
                Some(body),
                None,
            )
            .await?;

        Ok(CommitRevision::new(created.sha))
    }
}
