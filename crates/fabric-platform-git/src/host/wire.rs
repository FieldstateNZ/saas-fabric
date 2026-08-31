//! The request and response shapes of the calls this adapter makes.

/// What `GET /git/ref/heads/{branch}` answers with.
#[derive(Debug, serde::Deserialize)]
pub(super) struct RefObject {
    /// The commit the ref points at.
    pub(super) object: RefTarget,
}

/// The object a ref points at.
#[derive(Debug, serde::Deserialize)]
pub(super) struct RefTarget {
    /// The commit hash.
    pub(super) sha: String,
}

/// What `GET /contents/{path}` answers with for a file.
#[derive(Debug, serde::Deserialize)]
pub(super) struct ContentsFile {
    /// The blob hash, which becomes the file's revision.
    pub(super) sha: String,

    /// The file's content, base64 with line breaks in it.
    pub(super) content: String,
}

/// What `GET /git/commits/{sha}` answers with.
///
/// Only the tree is read. The commit's own hash is already known — it is what
/// was asked for — and a field nothing reads is a field that looks like a
/// contract and is not.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Commit {
    /// The tree it points at.
    pub(super) tree: TreeReference,
}

/// A reference to a tree.
#[derive(Debug, serde::Deserialize)]
pub(super) struct TreeReference {
    /// The tree hash.
    pub(super) sha: String,
}

/// What object creation answers with.
#[derive(Debug, serde::Deserialize)]
pub(super) struct Created {
    /// The hash of what was created.
    pub(super) sha: String,
}

/// A blob, as sent.
#[derive(Debug, serde::Serialize)]
pub(super) struct NewBlob<'a> {
    /// The file's text.
    pub(super) content: &'a str,

    /// Always `utf-8`. The contents API needs base64; this one does not, and
    /// sending text avoids an encode on the way out and a class of mistake
    /// with it.
    pub(super) encoding: &'static str,
}

/// A tree, as sent.
#[derive(Debug, serde::Serialize)]
pub(super) struct NewTree<'a> {
    /// The tree this one is layered on, so unlisted paths are inherited rather
    /// than deleted. Without it, a tree naming three files would be a commit
    /// deleting the entire rest of the repository.
    pub(super) base_tree: &'a str,

    /// The entries that differ from the base.
    pub(super) tree: Vec<TreeEntry<'a>>,
}

/// One entry in a tree being created.
#[derive(Debug, serde::Serialize)]
pub(super) struct TreeEntry<'a> {
    /// Repository-relative path.
    pub(super) path: &'a str,

    /// `100644` — a non-executable file. This adapter writes desired state,
    /// which is never a program.
    pub(super) mode: &'static str,

    /// Always `blob`.
    #[serde(rename = "type")]
    pub(super) kind: &'static str,

    /// The blob created for this path.
    pub(super) sha: &'a str,
}

/// A commit, as sent.
#[derive(Debug, serde::Serialize)]
pub(super) struct NewCommit<'a> {
    /// The commit message.
    pub(super) message: &'a str,

    /// The tree it points at.
    pub(super) tree: &'a str,

    /// Exactly one parent: the head this change was built on. It is what makes
    /// the ref update a fast-forward, and therefore what makes a lost race a
    /// `409` rather than a silent overwrite.
    pub(super) parents: Vec<&'a str>,
}

/// A ref update, as sent.
#[derive(Debug, serde::Serialize)]
pub(super) struct RefUpdateRequest<'a> {
    /// The commit to point the branch at.
    pub(super) sha: &'a str,

    /// **Always false.** There is no code path in this crate that sets it
    /// true, and this field is written out rather than omitted so that reading
    /// the request shape answers the question.
    pub(super) force: bool,
}
