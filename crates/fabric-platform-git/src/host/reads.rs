//! Reading the branch head, and files at a revision.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use reqwest::Method;

use crate::host::wire::{ContentsFile, RefObject};
use crate::host::PlatformGitRepository;
use crate::{CommitRevision, FileRevision, PlatformGitError, StoredFile};

impl PlatformGitRepository {
    /// The commit the integration branch currently points at.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformGitError`] if the branch is missing or the host could
    /// not be reached.
    pub async fn head(&self) -> Result<CommitRevision, PlatformGitError> {
        let branch = &self.config.branch;
        let url = self.url(&format!("git/ref/heads/{branch}"));

        let answer: RefObject = self
            .json("reading the branch head", Method::GET, url, None, Some(branch))
            .await?;

        Ok(CommitRevision::new(answer.object.sha))
    }

    /// Reads one file as it stands at a commit.
    ///
    /// Pinned to a revision rather than read from the branch, because the
    /// caller is about to decide what to write based on what it read, and "the
    /// branch" is a moving target between the two.
    ///
    /// # Errors
    ///
    /// Returns [`PlatformGitError::NotFound`] if there is no such file at that
    /// commit, and other variants if the host could not be reached.
    pub async fn read(&self, path: &str, at: &CommitRevision) -> Result<StoredFile, PlatformGitError> {
        let url = self.url(&format!("contents/{path}?ref={}", at.as_str()));

        let file: ContentsFile = self
            .json("reading a file", Method::GET, url, None, Some(path))
            .await?;

        Ok(StoredFile {
            path: path.to_owned(),
            text: decode(&file.content, path)?,
            revision: FileRevision::new(file.sha),
        })
    }

    /// The revision of one path at a commit, or `None` if it is not there.
    ///
    /// Absence is an answer rather than an error: the retry path asks "did
    /// this file move", and a file that has been deleted has moved.
    pub(crate) async fn revision_at(
        &self,
        path: &str,
        at: &CommitRevision,
    ) -> Result<Option<FileRevision>, PlatformGitError> {
        match self.read(path, at).await {
            Ok(file) => Ok(Some(file.revision)),
            Err(PlatformGitError::NotFound { .. }) => Ok(None),
            Err(other) => Err(other),
        }
    }
}

/// Decodes the base64 the contents API wraps a file in.
///
/// The host inserts line breaks, which the standard alphabet does not accept,
/// so they are stripped before decoding rather than the decoder being made
/// lenient about everything.
fn decode(content: &str, path: &str) -> Result<String, PlatformGitError> {
    let stripped: String = content
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();

    let bytes = BASE64
        .decode(stripped)
        .map_err(|_| PlatformGitError::Unavailable {
            detail: format!("{path} was not valid base64"),
        })?;

    String::from_utf8(bytes).map_err(|_| PlatformGitError::Unavailable {
        detail: format!("{path} was not valid UTF-8"),
    })
}
