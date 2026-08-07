//! Git object and ref storage for Tasker concurrency candidates.
//!
//! Candidate content is stored in Git objects.  Tasker remains the owner of
//! provenance, adjudication, and promotion state.  This crate deliberately
//! uses the system `git` executable instead of making a Git library part of
//! the Tasker dependency graph.
//!
//! The candidate API only creates, advances, reads, and removes refs in
//! [`CANDIDATE_REF_PREFIX`].  It never checks out a ref, changes the index, or
//! writes `refs/heads/*`.  [`GitCandidateAdapter::compare_and_swap_ref`] is a
//! lower-level primitive for the promotion reconciler and is intentionally
//! generic; callers responsible for canonical promotion may use it for a
//! canonical ref after their own authorization and revision checks.

use anyhow::{Context, Result, bail};
use jcode_tasker_types::{CandidateId, CandidateSetId};
use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fmt,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    str::FromStr,
};

/// The reserved namespace for candidate tips.
pub const CANDIDATE_REF_PREFIX: &str = "refs/tasker/candidates";

/// The companion namespace that records each candidate's immutable base OID.
///
/// Git cannot have both `refs/tasker/candidates/<set>/<candidate>` and a child
/// ref below that name, so base identities live in this sibling namespace.
pub const CANDIDATE_BASE_REF_PREFIX: &str = "refs/tasker/candidate-bases";

/// A typed, canonical candidate ref name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CandidateRef {
    candidate_set_id: CandidateSetId,
    candidate_id: CandidateId,
    name: String,
}

impl CandidateRef {
    /// Construct the canonical ref for a Tasker candidate.
    pub fn new(candidate_set_id: CandidateSetId, candidate_id: CandidateId) -> Self {
        let name = format!("{CANDIDATE_REF_PREFIX}/{candidate_set_id}/{candidate_id}");
        Self {
            candidate_set_id,
            candidate_id,
            name,
        }
    }

    /// Parse a candidate ref and reject non-canonical or out-of-namespace names.
    pub fn parse(value: &str) -> Result<Self> {
        let suffix = value
            .strip_prefix(&format!("{CANDIDATE_REF_PREFIX}/"))
            .with_context(|| format!("candidate ref is outside {CANDIDATE_REF_PREFIX}: {value}"))?;
        let mut components = suffix.split('/');
        let candidate_set_id = components
            .next()
            .context("candidate ref is missing its candidate-set id")?
            .parse::<CandidateSetId>()
            .context("candidate ref contains an invalid candidate-set id")?;
        let candidate_id = components
            .next()
            .context("candidate ref is missing its candidate id")?
            .parse::<CandidateId>()
            .context("candidate ref contains an invalid candidate id")?;
        if components.next().is_some() {
            bail!("candidate ref has more than one candidate path component: {value}");
        }

        let candidate_ref = Self::new(candidate_set_id, candidate_id);
        if candidate_ref.as_str() != value {
            bail!("candidate ref is not canonical: {value}");
        }
        Ok(candidate_ref)
    }

    /// Return the candidate-set identity encoded in this ref.
    pub const fn candidate_set_id(&self) -> CandidateSetId {
        self.candidate_set_id
    }

    /// Return the candidate identity encoded in this ref.
    pub const fn candidate_id(&self) -> CandidateId {
        self.candidate_id
    }

    /// Return the full Git ref name.
    pub fn as_str(&self) -> &str {
        &self.name
    }

    /// Return the companion ref that stores the candidate's base commit.
    pub fn base_ref_name(&self) -> String {
        format!(
            "{CANDIDATE_BASE_REF_PREFIX}/{}/{}",
            self.candidate_set_id, self.candidate_id
        )
    }
}

impl AsRef<str> for CandidateRef {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for CandidateRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for CandidateRef {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Optional identity used when creating a candidate commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitIdentity {
    pub name: String,
    pub email: String,
}

impl CommitIdentity {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
        }
    }
}

/// A tree and commit message to capture on a candidate ref.
///
/// The tree must already exist in the repository.  Producing that tree is
/// deliberately left to the candidate writer's isolated index/worktree.  The
/// adapter only creates the commit object and atomically advances the
/// candidate ref, so the caller's checked-out worktree is untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateChange {
    pub tree_oid: String,
    pub message: String,
    pub identity: Option<CommitIdentity>,
}

impl CandidateChange {
    pub fn new(tree_oid: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            tree_oid: tree_oid.into(),
            message: message.into(),
            identity: None,
        }
    }

    pub fn with_identity(mut self, identity: CommitIdentity) -> Self {
        self.identity = Some(identity);
        self
    }
}

/// Git identities and changed paths for a candidate tip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateMetadata {
    pub candidate_ref: CandidateRef,
    pub base_oid: String,
    pub tip_oid: String,
    pub changed_paths: Vec<String>,
}

/// The observed result of deleting a candidate's refs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    pub candidate_ref: CandidateRef,
    pub removed_candidate_ref: bool,
    pub removed_base_ref: bool,
    pub candidate_ref_remaining: bool,
    pub base_ref_remaining: bool,
}

/// The observed ref sets used to prove candidate/canonical isolation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsolationProof {
    pub candidate_refs: Vec<String>,
    pub canonical_refs: Vec<String>,
    pub overlaps: Vec<String>,
}

impl IsolationProof {
    pub fn is_isolated(&self) -> bool {
        self.overlaps.is_empty()
    }
}

/// A caller-scoped adapter over one Git repository.
#[derive(Debug, Clone)]
pub struct GitCandidateAdapter {
    repo_path: PathBuf,
}

/// Short alias for callers that prefer the noun-first name.
pub type CandidateGitAdapter = GitCandidateAdapter;

/// Short alias for the candidate Git store.
pub type CandidateGit = GitCandidateAdapter;

impl GitCandidateAdapter {
    /// Construct an adapter without running Git.  Use [`Self::try_new`] when
    /// the caller wants repository validation during construction.
    pub fn new(repo_path: impl AsRef<Path>) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
        }
    }

    /// Construct and verify that `repo_path` is a Git repository.
    pub fn try_new(repo_path: impl AsRef<Path>) -> Result<Self> {
        let adapter = Self::new(repo_path);
        adapter.git_stdout(["rev-parse", "--git-dir"])?;
        Ok(adapter)
    }

    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Create a candidate tip and base ref from a commit.
    ///
    /// Both refs are created in one Git ref transaction and both use an
    /// expected-old zero OID.  An existing candidate therefore fails closed
    /// rather than silently replacing candidate content.
    pub fn create_candidate_ref(
        &self,
        candidate_set_id: CandidateSetId,
        candidate_id: CandidateId,
        base: &str,
    ) -> Result<CandidateRef> {
        let candidate_ref = CandidateRef::new(candidate_set_id, candidate_id);
        let base_oid = self.resolve_commit(base)?;
        let zero_oid = self.zero_oid()?;
        let base_ref_name = candidate_ref.base_ref_name();
        self.update_refs_transaction(&[
            (candidate_ref.as_str(), &base_oid, &zero_oid),
            (&base_ref_name, &base_oid, &zero_oid),
        ])?;
        Ok(candidate_ref)
    }

    /// Alias for [`Self::create_candidate_ref`].
    pub fn create_candidate(
        &self,
        candidate_set_id: CandidateSetId,
        candidate_id: CandidateId,
        base: &str,
    ) -> Result<CandidateRef> {
        self.create_candidate_ref(candidate_set_id, candidate_id, base)
    }

    /// Materialize one detached worktree for a candidate ref.
    ///
    /// Worktrees are deliberately created below the caller-owned lane root and
    /// never use a branch name. The candidate ref remains the only mutable Git
    /// identity, so a candidate cannot advance a canonical branch by accident.
    pub fn create_candidate_worktree(
        &self,
        candidate_ref: &CandidateRef,
        path: impl AsRef<Path>,
    ) -> Result<PathBuf> {
        let path = path.as_ref();
        if path == self.repo_path {
            bail!("candidate worktree must not be the canonical repository path");
        }
        if path.exists() {
            bail!("candidate worktree path already exists: {}", path.display());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("create candidate worktree parent {}", parent.display())
            })?;
        }
        self.run_git_raw([
            "worktree",
            "add",
            "--detach",
            path.to_string_lossy().as_ref(),
            candidate_ref.as_str(),
        ])
        .with_context(|| format!("create candidate worktree {}", path.display()))?;
        if !path.is_dir() {
            bail!(
                "Git did not materialize candidate worktree {}",
                path.display()
            );
        }
        Ok(path.to_path_buf())
    }

    /// Remove a candidate worktree without touching any candidate refs.
    ///
    /// The operation is idempotent for an already-removed path, which lets
    /// timeout and cancellation cleanup safely retry after a partial failure.
    pub fn remove_candidate_worktree(&self, path: impl AsRef<Path>) -> Result<bool> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(false);
        }
        self.run_git_raw([
            "worktree",
            "remove",
            "--force",
            path.to_string_lossy().as_ref(),
        ])
        .with_context(|| format!("remove candidate worktree {}", path.display()))?;
        if path.exists() {
            bail!(
                "candidate worktree remains after cleanup: {}",
                path.display()
            );
        }
        Ok(true)
    }

    /// Remove a candidate worktree and its reserved refs as one host cleanup
    /// operation. Git ref deletion remains atomic; worktree removal is
    /// idempotent and is performed first so no process can keep the checkout
    /// alive after the lane becomes terminal.
    pub fn cleanup_candidate_worktree(
        &self,
        candidate_ref: &CandidateRef,
        path: impl AsRef<Path>,
    ) -> Result<CleanupReport> {
        self.remove_candidate_worktree(path)?;
        self.delete_candidate_ref(candidate_ref)
    }

    /// Read the base, tip, and changed paths for a candidate.
    pub fn read_candidate_metadata(
        &self,
        candidate_ref: &CandidateRef,
    ) -> Result<CandidateMetadata> {
        let base_ref_name = candidate_ref.base_ref_name();
        let base_oid = self
            .read_ref_oid(&base_ref_name)?
            .with_context(|| format!("candidate base ref is missing: {base_ref_name}"))?;
        let tip_oid = self
            .read_ref_oid(candidate_ref.as_str())?
            .with_context(|| format!("candidate ref is missing: {candidate_ref}"))?;
        let base_oid = self.resolve_commit(&base_oid)?;
        let tip_oid = self.resolve_commit(&tip_oid)?;
        let changed_paths = self.changed_paths(&base_oid, &tip_oid)?;

        Ok(CandidateMetadata {
            candidate_ref: candidate_ref.clone(),
            base_oid,
            tip_oid,
            changed_paths,
        })
    }

    /// Finalize a detached candidate worktree into its reserved candidate ref.
    ///
    /// A detached worktree advances `HEAD`, not the ref named at checkout.
    /// The host therefore performs the only ref update after verifying that
    /// the worktree is clean, its tip is a descendant of the reserved base,
    /// and the candidate ref still has the expected old tip.
    pub fn finalize_candidate_worktree(
        &self,
        candidate_ref: &CandidateRef,
        worktree_path: impl AsRef<Path>,
    ) -> Result<CandidateMetadata> {
        let worktree_path = worktree_path.as_ref();
        let expected_root = std::fs::canonicalize(worktree_path).with_context(|| {
            format!(
                "canonicalize candidate worktree {}",
                worktree_path.display()
            )
        })?;
        let registered_root = self.registered_worktree_root(&expected_root)?;
        if registered_root != expected_root {
            bail!(
                "candidate worktree {} is not the registered worktree",
                expected_root.display()
            );
        }
        let worktree_root = self.worktree_root(worktree_path)?;
        if worktree_root != expected_root {
            bail!(
                "candidate worktree resolved to {}, expected {}",
                worktree_root.display(),
                expected_root.display()
            );
        }

        let status = self.worktree_status(worktree_path)?;
        if !status.trim().is_empty() {
            bail!(
                "candidate worktree {} has uncommitted changes: {}",
                worktree_path.display(),
                status.trim()
            );
        }
        let worktree_tip = self.worktree_head(worktree_path)?;
        let current_tip = self
            .read_ref_oid(candidate_ref.as_str())?
            .with_context(|| format!("candidate ref is missing: {candidate_ref}"))?;
        let current_tip = self.resolve_commit(&current_tip)?;
        let worktree_tip = self.resolve_commit(&worktree_tip)?;

        let base_ref = candidate_ref.base_ref_name();
        let base_oid = self
            .read_ref_oid(&base_ref)?
            .with_context(|| format!("candidate base ref is missing: {base_ref}"))?;
        let base_oid = self.resolve_commit(&base_oid)?;
        if !self.is_ancestor(&base_oid, &worktree_tip)? {
            bail!(
                "candidate worktree tip {} is not based on reserved base {}",
                worktree_tip,
                base_oid
            );
        }

        if current_tip != worktree_tip {
            self.compare_and_swap_candidate_ref(candidate_ref, &current_tip, &worktree_tip)?;
        }

        let metadata = self.read_candidate_metadata(candidate_ref)?;
        if !self.is_ancestor(&metadata.base_oid, &metadata.tip_oid)? {
            bail!(
                "candidate tip {} is not based on reserved base {}",
                metadata.tip_oid,
                metadata.base_oid
            );
        }
        Ok(metadata)
    }

    /// Derive a stable digest from the committed candidate diff.
    pub fn candidate_diff_digest(&self, candidate_ref: &CandidateRef) -> Result<String> {
        let metadata = self.read_candidate_metadata(candidate_ref)?;
        let diff = self.git_output_bytes([
            "diff",
            "--binary",
            "--full-index",
            "--no-ext-diff",
            "--no-renames",
            &metadata.base_oid,
            &metadata.tip_oid,
            "--",
        ])?;
        Ok(format!("sha256:{:x}", Sha256::digest(diff)))
    }

    /// Read the HEAD of the registered candidate worktree after validating
    /// that the supplied path is the worktree Git registered for this repo.
    pub fn read_worktree_head(&self, worktree_path: impl AsRef<Path>) -> Result<String> {
        let worktree_path = worktree_path.as_ref();
        let expected_root = std::fs::canonicalize(worktree_path).with_context(|| {
            format!(
                "canonicalize candidate worktree {}",
                worktree_path.display()
            )
        })?;
        self.registered_worktree_root(&expected_root)?;
        let worktree_root = self.worktree_root(worktree_path)?;
        if worktree_root != expected_root {
            bail!(
                "candidate worktree resolved to {}, expected {}",
                worktree_root.display(),
                expected_root.display()
            );
        }
        self.worktree_head(worktree_path)
    }

    /// Verify that a candidate worktree has no staged, unstaged, or untracked
    /// changes after host validation commands have run.
    pub fn ensure_worktree_clean(&self, worktree_path: impl AsRef<Path>) -> Result<()> {
        let worktree_path = worktree_path.as_ref();
        let status = self.worktree_status(worktree_path)?;
        if status.trim().is_empty() {
            Ok(())
        } else {
            bail!(
                "candidate worktree {} has uncommitted changes: {}",
                worktree_path.display(),
                status.trim()
            )
        }
    }

    /// Read metadata by candidate-set and candidate identity.
    pub fn candidate_metadata(
        &self,
        candidate_set_id: CandidateSetId,
        candidate_id: CandidateId,
    ) -> Result<CandidateMetadata> {
        self.read_candidate_metadata(&CandidateRef::new(candidate_set_id, candidate_id))
    }

    /// Create a commit from an existing tree and atomically advance a candidate ref.
    pub fn capture_change(
        &self,
        candidate_ref: &CandidateRef,
        tree_oid: &str,
        message: &str,
    ) -> Result<String> {
        self.capture_candidate_change(candidate_ref, &CandidateChange::new(tree_oid, message))
    }

    /// Capture a change with an explicit author and committer identity.
    pub fn capture_candidate_change(
        &self,
        candidate_ref: &CandidateRef,
        change: &CandidateChange,
    ) -> Result<String> {
        let current_tip = self
            .read_ref_oid(candidate_ref.as_str())?
            .with_context(|| format!("candidate ref is missing: {candidate_ref}"))?;
        let current_tip = self.resolve_commit(&current_tip)?;
        let tree_oid = self.resolve_tree(&change.tree_oid)?;
        let commit_oid = self.commit_tree(
            &tree_oid,
            &current_tip,
            &change.message,
            change.identity.as_ref(),
        )?;

        // The commit object may be left unreachable if another writer wins the
        // race.  That is harmless and is exactly what Git's normal GC policy
        // is designed to collect; the candidate ref itself is never replaced.
        self.compare_and_swap_ref(candidate_ref.as_str(), &current_tip, &commit_oid)?;
        Ok(commit_oid)
    }

    /// Alias emphasizing that the input is a precomputed tree.
    pub fn capture_tree(
        &self,
        candidate_ref: &CandidateRef,
        tree_oid: &str,
        message: &str,
    ) -> Result<String> {
        self.capture_change(candidate_ref, tree_oid, message)
    }

    /// Compare-and-swap a Git ref using `git update-ref`.
    ///
    /// This is the primitive used by promotion code.  It accepts arbitrary
    /// fully-qualified refs, including `refs/heads/*`, because the promotion
    /// reconciler owns the authorization boundary.  Candidate operations in
    /// this crate only call it with refs from [`CANDIDATE_REF_PREFIX`].
    pub fn compare_and_swap_ref(
        &self,
        ref_name: &str,
        expected_old_oid: &str,
        new_oid: &str,
    ) -> Result<()> {
        self.validate_ref_name(ref_name)?;
        let expected_old_oid = self.normalize_update_oid(expected_old_oid)?;
        let new_oid = self.normalize_update_oid(new_oid)?;
        self.update_refs_transaction(&[(ref_name, &new_oid, &expected_old_oid)])
    }

    /// Compare-and-swap a candidate tip without requiring callers to format its ref.
    pub fn compare_and_swap_candidate_ref(
        &self,
        candidate_ref: &CandidateRef,
        expected_old_oid: &str,
        new_oid: &str,
    ) -> Result<()> {
        self.compare_and_swap_ref(candidate_ref.as_str(), expected_old_oid, new_oid)
    }

    /// Delete a candidate tip and its base identity atomically.
    ///
    /// Deleting refs makes their commits eligible for Git's ordinary garbage
    /// collection without forcing a repository-wide `git gc` or touching the
    /// checked-out worktree.  The operation is idempotent when both refs are
    /// already absent.
    pub fn delete_candidate_ref(&self, candidate_ref: &CandidateRef) -> Result<CleanupReport> {
        let base_ref_name = candidate_ref.base_ref_name();
        let candidate_oid = self.read_ref_oid(candidate_ref.as_str())?;
        let base_oid = self.read_ref_oid(&base_ref_name)?;
        let removed_candidate_ref = candidate_oid.is_some();
        let removed_base_ref = base_oid.is_some();

        let zero_oid = self.zero_oid()?;
        let mut updates = Vec::with_capacity(2);
        if let Some(candidate_oid) = candidate_oid.as_deref() {
            updates.push((candidate_ref.as_str(), zero_oid.as_str(), candidate_oid));
        }
        if let Some(base_oid) = base_oid.as_deref() {
            updates.push((base_ref_name.as_str(), zero_oid.as_str(), base_oid));
        }
        if !updates.is_empty() {
            self.update_refs_transaction(&updates)?;
        }

        let candidate_ref_remaining = self.read_ref_oid(candidate_ref.as_str())?.is_some();
        let base_ref_remaining = self.read_ref_oid(&base_ref_name)?.is_some();
        if candidate_ref_remaining || base_ref_remaining {
            bail!("candidate cleanup did not remove all refs for {candidate_ref}");
        }

        Ok(CleanupReport {
            candidate_ref: candidate_ref.clone(),
            removed_candidate_ref,
            removed_base_ref,
            candidate_ref_remaining,
            base_ref_remaining,
        })
    }

    /// Alias for cleanup jobs that explicitly handle abandoned candidates.
    pub fn cleanup_abandoned_candidate(
        &self,
        candidate_ref: &CandidateRef,
    ) -> Result<CleanupReport> {
        self.delete_candidate_ref(candidate_ref)
    }

    /// List all well-formed candidate tip refs in the reserved namespace.
    pub fn list_candidate_refs(&self) -> Result<Vec<CandidateRef>> {
        self.list_candidate_ref_names()?
            .iter()
            .map(|name| CandidateRef::parse(name))
            .collect()
    }

    /// List candidate ref names without interpreting their IDs.
    pub fn list_candidate_ref_names(&self) -> Result<Vec<String>> {
        let prefix = format!("{CANDIDATE_REF_PREFIX}/");
        Ok(self
            .list_refs(CANDIDATE_REF_PREFIX)?
            .into_iter()
            .filter(|name| {
                let Some(suffix) = name.strip_prefix(&prefix) else {
                    return false;
                };
                suffix.split('/').count() == 2
            })
            .collect())
    }

    /// Observe candidate and canonical refs and prove that their names do not overlap.
    pub fn prove_isolation(&self) -> Result<IsolationProof> {
        let candidate_refs = self.list_candidate_ref_names()?;
        let canonical_refs = self.list_refs("refs/heads")?;
        let overlaps = candidate_refs
            .iter()
            .filter(|candidate_ref| canonical_refs.binary_search(candidate_ref).is_ok())
            .cloned()
            .collect::<Vec<_>>();
        Ok(IsolationProof {
            candidate_refs,
            canonical_refs,
            overlaps,
        })
    }

    /// Return an isolation proof or fail if a candidate ref appears in heads.
    pub fn assert_isolated(&self) -> Result<IsolationProof> {
        let proof = self.prove_isolation()?;
        if !proof.is_isolated() {
            bail!(
                "candidate refs overlap canonical refs: {:?}",
                proof.overlaps
            );
        }
        Ok(proof)
    }

    fn changed_paths(&self, base_oid: &str, tip_oid: &str) -> Result<Vec<String>> {
        let output = self.git_output_bytes([
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            base_oid,
            tip_oid,
            "--",
        ])?;
        let mut paths = output
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
            .map(|path| String::from_utf8_lossy(path).into_owned())
            .collect::<Vec<_>>();
        paths.sort();
        paths.dedup();
        Ok(paths)
    }

    fn registered_worktree_root(&self, expected_root: &Path) -> Result<PathBuf> {
        let output = self.git_stdout(["worktree", "list", "--porcelain"])?;
        for line in output.lines() {
            if let Some(path) = line.strip_prefix("worktree ") {
                let path = std::fs::canonicalize(path)?;
                if path == expected_root {
                    return Ok(expected_root.to_path_buf());
                }
            }
        }
        bail!(
            "candidate worktree {} is not registered by Git",
            expected_root.display()
        )
    }

    fn worktree_root(&self, path: &Path) -> Result<PathBuf> {
        let root = self.run_git_at(path, ["rev-parse", "--show-toplevel"])?;
        Ok(std::fs::canonicalize(
            String::from_utf8(root.stdout)
                .context("candidate worktree returned non-UTF-8 root")?
                .trim(),
        )?)
    }

    fn worktree_head(&self, path: &Path) -> Result<String> {
        let output = self.run_git_at(path, ["rev-parse", "--verify", "HEAD"])?;
        Ok(String::from_utf8(output.stdout)
            .context("candidate worktree returned non-UTF-8 HEAD")?
            .trim()
            .to_owned())
    }

    fn worktree_status(&self, path: &Path) -> Result<String> {
        let output =
            self.run_git_at(path, ["status", "--porcelain=v1", "--untracked-files=all"])?;
        Ok(String::from_utf8(output.stdout)
            .context("candidate worktree returned non-UTF-8 status")?)
    }

    fn is_ancestor(&self, base_oid: &str, tip_oid: &str) -> Result<bool> {
        let output = self.run_git_raw(["merge-base", "--is-ancestor", base_oid, tip_oid])?;
        if output.status.success() {
            Ok(true)
        } else if output.status.code() == Some(1) {
            Ok(false)
        } else {
            bail!(
                "git merge-base --is-ancestor failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )
        }
    }

    fn commit_tree(
        &self,
        tree_oid: &str,
        parent_oid: &str,
        message: &str,
        identity: Option<&CommitIdentity>,
    ) -> Result<String> {
        let mut command = self.git_command();
        command.args(["commit-tree", tree_oid, "-p", parent_oid, "-m", message]);
        if let Some(identity) = identity {
            command
                .env("GIT_AUTHOR_NAME", &identity.name)
                .env("GIT_AUTHOR_EMAIL", &identity.email)
                .env("GIT_COMMITTER_NAME", &identity.name)
                .env("GIT_COMMITTER_EMAIL", &identity.email);
        }
        let output = self.run_command(command, "git commit-tree")?;
        let commit_oid = String::from_utf8(output.stdout)
            .context("git commit-tree returned non-UTF-8 output")?
            .trim()
            .to_string();
        if commit_oid.is_empty() {
            bail!("git commit-tree returned an empty commit OID");
        }
        Ok(commit_oid)
    }

    fn resolve_commit(&self, value: &str) -> Result<String> {
        self.resolve_object(value, "commit")
    }

    fn resolve_tree(&self, value: &str) -> Result<String> {
        self.resolve_object(value, "tree")
    }

    fn resolve_object(&self, value: &str, kind: &str) -> Result<String> {
        let revision = format!("{value}^{{{kind}}}");
        self.git_stdout(["rev-parse", "--verify", "--end-of-options", &revision])
    }

    fn normalize_update_oid(&self, value: &str) -> Result<String> {
        if value.is_empty() || value.chars().all(|character| character == '0') {
            return self.zero_oid();
        }
        self.resolve_object(value, "object")
    }

    fn zero_oid(&self) -> Result<String> {
        let format = self
            .git_stdout(["rev-parse", "--show-object-format"])?
            .trim()
            .to_string();
        let length = match format.as_str() {
            "sha1" => 40,
            "sha256" => 64,
            other => bail!("unsupported Git object format: {other}"),
        };
        Ok("0".repeat(length))
    }

    fn validate_ref_name(&self, ref_name: &str) -> Result<()> {
        if !ref_name.starts_with("refs/") {
            bail!("Git ref must be fully qualified below refs/: {ref_name}");
        }
        self.git_stdout(["check-ref-format", ref_name])?;
        Ok(())
    }

    fn update_refs_transaction(&self, updates: &[(&str, &str, &str)]) -> Result<()> {
        let mut input = String::from("start\n");
        for (ref_name, new_oid, old_oid) in updates {
            self.validate_ref_name(ref_name)?;
            input.push_str("update ");
            input.push_str(ref_name);
            input.push(' ');
            input.push_str(new_oid);
            input.push(' ');
            input.push_str(old_oid);
            input.push('\n');
        }
        input.push_str("prepare\ncommit\n");
        self.run_git_stdin(["update-ref", "--no-deref", "--stdin"], input.as_bytes())?;
        Ok(())
    }

    fn read_ref_oid(&self, ref_name: &str) -> Result<Option<String>> {
        self.validate_ref_name(ref_name)?;
        let output = self.run_git_raw(["show-ref", "--hash", "--verify", ref_name])?;
        if output.status.success() {
            let oid = String::from_utf8(output.stdout)
                .context("git show-ref returned non-UTF-8 output")?
                .trim()
                .to_string();
            if oid.is_empty() {
                bail!("git show-ref returned an empty OID for {ref_name}");
            }
            return Ok(Some(oid));
        }
        if output.stdout.is_empty() {
            return Ok(None);
        }
        bail!(
            "git show-ref failed for {ref_name}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    fn list_refs(&self, prefix: &str) -> Result<Vec<String>> {
        let mut refs = self
            .git_stdout(["for-each-ref", "--format=%(refname)", prefix])?
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        refs.sort();
        refs.dedup();
        Ok(refs)
    }

    fn git_command(&self) -> Command {
        let mut command = Command::new("git");
        command.current_dir(&self.repo_path);
        command
    }

    fn run_git_at<I, S>(&self, path: &Path, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr> + Clone,
    {
        let args = args.into_iter().collect::<Vec<_>>();
        let rendered = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let mut command = Command::new("git");
        command.current_dir(path).args(&args);
        self.run_command(command, &format!("git -C {} {rendered}", path.display()))
    }

    fn run_git_raw<I, S>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr> + Clone,
    {
        let args = args.into_iter().collect::<Vec<_>>();
        let rendered = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let mut command = self.git_command();
        command.args(&args);
        self.run_command_raw(command, &format!("git {rendered}"))
    }

    fn run_git_stdin<I, S>(&self, args: I, input: &[u8]) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr> + Clone,
    {
        let args = args.into_iter().collect::<Vec<_>>();
        let rendered = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let mut command = self.git_command();
        command
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to spawn git {rendered}"))?;
        child
            .stdin
            .take()
            .context("git update-ref did not expose stdin")?
            .write_all(input)
            .context("failed to write Git ref transaction")?;
        let output = child
            .wait_with_output()
            .with_context(|| format!("failed waiting for git {rendered}"))?;
        if !output.status.success() {
            bail!(
                "git {rendered} failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }

    fn run_command_raw(&self, mut command: Command, description: &str) -> Result<Output> {
        command
            .output()
            .with_context(|| format!("failed to execute {description}"))
    }

    fn run_command(&self, command: Command, description: &str) -> Result<Output> {
        let output = self.run_command_raw(command, description)?;
        if !output.status.success() {
            bail!(
                "{description} failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }

    fn git_stdout<I, S>(&self, args: I) -> Result<String>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr> + Clone,
    {
        let output = self.run_git_checked(args)?;
        Ok(String::from_utf8(output.stdout)
            .context("Git returned non-UTF-8 stdout")?
            .trim()
            .to_owned())
    }

    fn git_output_bytes<I, S>(&self, args: I) -> Result<Vec<u8>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr> + Clone,
    {
        Ok(self.run_git_checked(args)?.stdout)
    }

    fn run_git_checked<I, S>(&self, args: I) -> Result<Output>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr> + Clone,
    {
        let args = args.into_iter().collect::<Vec<_>>();
        let rendered = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        let output = self.run_git_raw(args)?;
        if !output.status.success() {
            bail!(
                "git {rendered} failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    struct ScratchRepo {
        path: PathBuf,
    }

    impl Drop for ScratchRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    impl ScratchRepo {
        fn new() -> Result<Self> {
            let root = std::env::temp_dir();
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock predates Unix epoch")?
                .as_nanos();
            let path = root.join(format!(
                "jcode-tasker-git-{}-{timestamp}",
                std::process::id()
            ));
            fs::create_dir(&path)
                .with_context(|| format!("failed to create scratch repo {}", path.display()))?;
            run_git(&path, &["init", "--quiet"], None)?;
            run_git(&path, &["config", "user.name", "Tasker Test"], None)?;
            run_git(
                &path,
                &["config", "user.email", "tasker-test@example.invalid"],
                None,
            )?;
            fs::write(path.join("base.txt"), "base\n")?;
            run_git(&path, &["add", "base.txt"], None)?;
            run_git(&path, &["commit", "--quiet", "-m", "base"], None)?;
            Ok(Self { path })
        }

        fn base_oid(&self) -> Result<String> {
            run_git(&self.path, &["rev-parse", "HEAD"], None)
        }

        fn head_refs(&self) -> Result<Vec<String>> {
            let output = run_git(
                &self.path,
                &["for-each-ref", "--format=%(refname)", "refs/heads"],
                None,
            )?;
            Ok(output.lines().map(ToOwned::to_owned).collect())
        }

        fn worktree_state(&self) -> Result<(String, String)> {
            let head = run_git(&self.path, &["rev-parse", "HEAD"], None)?;
            let status = run_git(&self.path, &["status", "--porcelain=v1"], None)?;
            Ok((head, status))
        }

        fn changed_tree(&self, base_oid: &str) -> Result<String> {
            let index_path = self.path.join("tasker-test-index");
            let index = index_path.to_string_lossy().into_owned();
            run_git_with_env(
                &self.path,
                &["read-tree", base_oid],
                None,
                &[("GIT_INDEX_FILE", &index)],
            )?;
            let blob_oid = run_git_with_env(
                &self.path,
                &["hash-object", "-w", "--stdin"],
                Some(b"candidate\n"),
                &[("GIT_INDEX_FILE", &index)],
            )?;
            let cacheinfo = format!("100644,{blob_oid},candidate.txt");
            run_git_with_env(
                &self.path,
                &["update-index", "--add", "--cacheinfo", &cacheinfo],
                None,
                &[("GIT_INDEX_FILE", &index)],
            )?;
            let tree = run_git_with_env(
                &self.path,
                &["write-tree"],
                None,
                &[("GIT_INDEX_FILE", &index)],
            )?;
            let _ = fs::remove_file(index_path);
            Ok(tree)
        }
    }

    fn run_git(repo: &Path, args: &[&str], input: Option<&[u8]>) -> Result<String> {
        run_git_with_env(repo, args, input, &[])
    }

    fn run_git_with_env(
        repo: &Path,
        args: &[&str],
        input: Option<&[u8]>,
        env: &[(&str, &str)],
    ) -> Result<String> {
        let mut command = Command::new("git");
        command
            .current_dir(repo)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            command.env(key, value);
        }
        if input.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command
            .spawn()
            .with_context(|| format!("spawn git {}", args.join(" ")))?;
        if let Some(input) = input {
            child
                .stdin
                .take()
                .context("scratch git child did not expose stdin")?
                .write_all(input)?;
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    #[test]
    fn candidate_ref_round_trips_and_stays_in_reserved_namespace() -> Result<()> {
        let candidate_ref = CandidateRef::new(CandidateSetId::new(), CandidateId::new());
        assert!(
            candidate_ref
                .as_str()
                .starts_with("refs/tasker/candidates/")
        );
        assert_eq!(CandidateRef::parse(candidate_ref.as_str())?, candidate_ref);
        assert!(CandidateRef::parse("refs/heads/main").is_err());
        Ok(())
    }

    #[test]
    fn candidate_commit_metadata_and_isolation_do_not_touch_worktree_or_heads() -> Result<()> {
        let repo = ScratchRepo::new()?;
        let adapter = GitCandidateAdapter::try_new(&repo.path)?;
        let base_oid = repo.base_oid()?;
        let before = repo.worktree_state()?;
        let before_heads = repo.head_refs()?;
        let candidate_set_id = CandidateSetId::new();
        let candidate_id = CandidateId::new();
        let candidate_ref =
            adapter.create_candidate_ref(candidate_set_id, candidate_id, &base_oid)?;
        let tree_oid = repo.changed_tree(&base_oid)?;
        let tip_oid = adapter.capture_change(&candidate_ref, &tree_oid, "candidate change")?;

        let metadata = adapter.read_candidate_metadata(&candidate_ref)?;
        assert_eq!(metadata.base_oid, base_oid);
        assert_eq!(metadata.tip_oid, tip_oid);
        assert_eq!(metadata.changed_paths, vec!["candidate.txt"]);

        let proof = adapter.assert_isolated()?;
        assert!(proof.is_isolated());
        assert!(proof.candidate_refs.contains(&candidate_ref.to_string()));
        assert!(
            !proof
                .canonical_refs
                .iter()
                .any(|name| name == candidate_ref.as_str())
        );
        assert_eq!(repo.worktree_state()?, before);
        assert_eq!(repo.head_refs()?, before_heads);
        Ok(())
    }

    #[test]
    fn finalizing_detached_worktree_advances_only_candidate_ref() -> Result<()> {
        let repo = ScratchRepo::new()?;
        let adapter = GitCandidateAdapter::try_new(&repo.path)?;
        let base_oid = repo.base_oid()?;
        let before_heads = repo.head_refs()?;
        let candidate_ref =
            adapter.create_candidate_ref(CandidateSetId::new(), CandidateId::new(), &base_oid)?;
        let worktree = repo.path.join("candidate-worktree");
        adapter.create_candidate_worktree(&candidate_ref, &worktree)?;

        fs::write(worktree.join("candidate.txt"), "candidate\n")?;
        run_git(&worktree, &["add", "candidate.txt"], None)?;
        run_git(&worktree, &["commit", "--quiet", "-m", "candidate"], None)?;
        let detached_head = run_git(&worktree, &["rev-parse", "HEAD"], None)?;
        assert_eq!(
            adapter.read_candidate_metadata(&candidate_ref)?.tip_oid,
            base_oid
        );

        let metadata = adapter.finalize_candidate_worktree(&candidate_ref, &worktree)?;
        assert_eq!(metadata.tip_oid, detached_head);
        assert_eq!(metadata.base_oid, base_oid);
        assert_eq!(metadata.changed_paths, vec!["candidate.txt"]);
        assert!(
            adapter
                .candidate_diff_digest(&candidate_ref)?
                .starts_with("sha256:")
        );
        assert_eq!(repo.head_refs()?, before_heads);
        assert_eq!(run_git(&repo.path, &["rev-parse", "HEAD"], None)?, base_oid);
        assert!(run_git(&worktree, &["status", "--porcelain"], None)?.is_empty());

        adapter.cleanup_candidate_worktree(&candidate_ref, &worktree)?;
        Ok(())
    }

    #[test]
    fn divergent_detached_worktree_does_not_advance_candidate_ref() -> Result<()> {
        let repo = ScratchRepo::new()?;
        let adapter = GitCandidateAdapter::try_new(&repo.path)?;
        let base_oid = repo.base_oid()?;
        let candidate_ref =
            adapter.create_candidate_ref(CandidateSetId::new(), CandidateId::new(), &base_oid)?;
        let worktree = repo.path.join("divergent-worktree");
        adapter.create_candidate_worktree(&candidate_ref, &worktree)?;

        let tree_oid = run_git(&worktree, &["mktree"], Some(b""))?;
        let orphan_oid = run_git(
            &worktree,
            &["commit-tree", &tree_oid, "-m", "orphan candidate"],
            None,
        )?;
        run_git(&worktree, &["reset", "--hard", &orphan_oid], None)?;

        let result = adapter.finalize_candidate_worktree(&candidate_ref, &worktree);
        assert!(result.is_err());
        assert_eq!(
            adapter.read_candidate_metadata(&candidate_ref)?.tip_oid,
            base_oid
        );

        adapter.cleanup_candidate_worktree(&candidate_ref, &worktree)?;
        Ok(())
    }

    #[test]
    fn compare_and_swap_succeeds_then_rejects_a_stale_expected_oid() -> Result<()> {
        let repo = ScratchRepo::new()?;
        let adapter = GitCandidateAdapter::try_new(&repo.path)?;
        let base_oid = repo.base_oid()?;
        let candidate_ref =
            adapter.create_candidate_ref(CandidateSetId::new(), CandidateId::new(), &base_oid)?;
        let tree_oid = repo.changed_tree(&base_oid)?;
        let tip_oid = adapter.capture_change(&candidate_ref, &tree_oid, "candidate change")?;

        adapter.compare_and_swap_ref(candidate_ref.as_str(), &tip_oid, &base_oid)?;
        let stale = adapter.compare_and_swap_ref(candidate_ref.as_str(), &tip_oid, &tip_oid);
        assert!(stale.is_err());
        assert_eq!(
            adapter.read_candidate_metadata(&candidate_ref)?.tip_oid,
            base_oid
        );
        Ok(())
    }

    #[test]
    fn cleanup_removes_candidate_refs_and_leaves_objects_for_normal_git_gc() -> Result<()> {
        let repo = ScratchRepo::new()?;
        let adapter = GitCandidateAdapter::try_new(&repo.path)?;
        let base_oid = repo.base_oid()?;
        let candidate_ref =
            adapter.create_candidate_ref(CandidateSetId::new(), CandidateId::new(), &base_oid)?;
        let tree_oid = repo.changed_tree(&base_oid)?;
        let tip_oid = adapter.capture_change(&candidate_ref, &tree_oid, "abandoned candidate")?;
        assert!(
            adapter
                .list_candidate_ref_names()?
                .contains(&candidate_ref.to_string())
        );

        let report = adapter.cleanup_abandoned_candidate(&candidate_ref)?;
        assert!(report.removed_candidate_ref);
        assert!(report.removed_base_ref);
        assert!(!report.candidate_ref_remaining);
        assert!(!report.base_ref_remaining);
        assert!(
            !adapter
                .list_candidate_ref_names()?
                .contains(&candidate_ref.to_string())
        );
        assert!(
            run_git(
                &repo.path,
                &["show-ref", "--verify", candidate_ref.as_str()],
                None
            )
            .is_err()
        );
        assert!(
            run_git(
                &repo.path,
                &["show-ref", "--verify", &candidate_ref.base_ref_name()],
                None
            )
            .is_err()
        );
        assert!(run_git(&repo.path, &["cat-file", "-t", &tip_oid], None).is_ok());
        Ok(())
    }
}
