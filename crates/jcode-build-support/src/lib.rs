pub mod fork_governance;
mod paths;
mod platform_support;
mod source_state;
mod storage_helpers;

pub use paths::{
    SELFDEV_CARGO_PROFILE, binary_name, binary_stem, client_update_candidate,
    current_binary_build_time_string, current_binary_built_at, find_dev_binary,
    find_repo_in_ancestors, get_repo_dir, is_jcode_repo, launcher_binary_path, launcher_dir,
    preferred_reload_candidate, release_binary_path, resolve_binary_payload, run_selfdev_build,
    selfdev_binary_path, selfdev_build_command, selfdev_build_command_for_target,
    shared_server_update_candidate, update_launcher_symlink_to_current,
    update_launcher_symlink_to_stable, version_matches_installed_channel,
};
pub use source_state::{
    current_build_info, current_git_diff, current_git_hash, current_git_hash_full,
    current_source_state, ensure_source_state_matches, get_commit_message, is_working_tree_dirty,
    repo_build_version, repo_scope_key, worktree_scope_key,
};
pub use storage_helpers::{
    build_log_path, build_progress_path, builds_dir, canary_binary_path, clear_build_progress,
    clear_migration_context, current_binary_path, current_version_file, load_migration_context,
    manifest_path, migration_context_path, read_build_progress, read_current_version,
    read_shared_server_version, read_stable_version, save_migration_context,
    shared_server_binary_path, shared_server_version_file, stable_binary_path, stable_version_file,
    version_binary_path, write_build_progress,
};

use anyhow::{Context, Result};
use chrono::Utc;
use jcode_storage as storage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::time::{Duration, Instant};

pub use fork_governance::{
    CapabilityManifestEntry, CapabilityRelationship, ContractSpec, ForkCapabilityManifest,
    LEGACY_MANIFEST_SCHEMA_VERSION, MANIFEST_FILE_NAME, MANIFEST_SCHEMA_VERSION,
    RECOVERY_BASELINE_SCHEMA_VERSION, RecoveryBaseline, RetirementRecord, RuntimeSmokeSpec,
    builtin_capability_ids, builtin_manifest, digest_bytes, load_manifest_or_legacy,
    manifest_path_for_binary, read_manifest_for_binary, write_immutable_manifest,
};
pub use jcode_selfdev_types::{
    BinaryChoice, BinaryVersionReport, BuildInfo, CanaryStatus, CrashInfo, DevBinarySourceMetadata,
    MigrationContext, PendingActivation, PublishedBuild, SelfDevBuildCommand, SelfDevBuildTarget,
    SourceState,
};

/// GitHub repository whose release artifacts are allowed to become executable
/// channel builds. This is deliberately the maintained fork, not the upstream
/// source repository.
pub const CANONICAL_FORK_REPOSITORY: &str = "creatifcoding/jcode";
/// Branch on the canonical fork that is allowed to feed automatic source
/// updates. Upstream branches are intake-only and never build targets.
pub const CANONICAL_FORK_RELEASE_BRANCH: &str = "master";
/// Upstream is an input to source-update workflows only. It is never a valid
/// release or local-fork binary authority.
pub const UPSTREAM_SOURCE_REPOSITORY: &str = "1jehuang/jcode";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestTrust {
    /// A normal admission must carry the current frozen manifest contract.
    #[default]
    Modern,
    /// The host proved a historical pre-baseline migration before recording
    /// this receipt. This is never inferred from a candidate sidecar.
    TrustedLegacyMigration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForkBuildAdmission {
    pub version: String,
    pub git_hash: String,
    pub authority: String,
    /// SHA-256 of the immutable binary payload admitted for this version.
    pub binary_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<String>,
    /// Source-state fingerprint for local/dirty builds. Release assets use
    /// the canonical release metadata and therefore have no dirty fingerprint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_fingerprint: Option<String>,
    /// Git provenance for an automatic source build. This is recorded only
    /// after the checkout's remote, release branch, and ancestry have been
    /// verified locally. A caller-supplied authority string is not evidence of
    /// this provenance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ForkBuildSource>,
    /// Host-owned classification of the manifest path used for admission.
    /// Receipts written before this field existed default to modern, which is
    /// intentionally fail-closed for legacy-shaped binaries.
    #[serde(default)]
    pub manifest_trust: ManifestTrust,
    pub capabilities: Vec<String>,
    pub admitted_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForkBuildSource {
    pub repository: String,
    pub branch: String,
    pub commit: String,
    /// Upstream commit that was fetched as intake and then incorporated by a
    /// verified merge. The upstream commit is never itself a build target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intake_commit: Option<String>,
}

fn canonical_authority(authority: &str) -> bool {
    authority.starts_with(&format!("github:{CANONICAL_FORK_REPOSITORY}:"))
        || authority.starts_with(&format!("local-fork:{CANONICAL_FORK_REPOSITORY}:"))
}

fn source_intake_authority(authority: &str) -> bool {
    authority == format!("local-fork:{CANONICAL_FORK_REPOSITORY}:source-intake")
}

fn canonical_remote_url(url: &str) -> bool {
    let url = url.trim().trim_end_matches('/');
    [
        format!("https://github.com/{CANONICAL_FORK_REPOSITORY}"),
        format!("git@github.com:{CANONICAL_FORK_REPOSITORY}"),
        format!("ssh://git@github.com/{CANONICAL_FORK_REPOSITORY}"),
    ]
    .into_iter()
    .any(|prefix| url.trim_end_matches(".git") == prefix)
}

fn git_output(repo_dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .with_context(|| format!("failed to run git {}", args.join(" ")))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_ref_exists(repo_dir: &Path, reference: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", reference])
        .current_dir(repo_dir)
        .output()
        .with_context(|| format!("failed to check git ref {reference}"))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => anyhow::bail!(
            "failed to check git ref {reference}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

fn git_is_ancestor(repo_dir: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    let output = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(repo_dir)
        .output()
        .with_context(|| format!("failed to compare git ancestry {ancestor}..{descendant}"))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => anyhow::bail!(
            "failed to compare git ancestry {ancestor}..{descendant}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    }
}

fn is_hex_commit(value: &str) -> bool {
    let value = value.trim();
    (7..=64).contains(&value.len()) && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn commit_matches(expected: &str, actual: &str) -> bool {
    let expected = expected.trim();
    let actual = actual.trim();
    !expected.is_empty()
        && !actual.is_empty()
        && (expected == actual || actual.starts_with(expected) || expected.starts_with(actual))
}

/// Return the configured remote that points at the canonical fork.
///
/// This deliberately rejects a checkout whose only remote is upstream. The
/// updater may fetch upstream as intake in a separate workflow, but it must
/// never turn that checkout into a build target by relabeling it.
pub fn canonical_fork_remote(repo_dir: &Path) -> Result<String> {
    let remotes = git_output(repo_dir, &["remote"])?;
    for remote in remotes
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let url = git_output(repo_dir, &["remote", "get-url", remote])?;
        if canonical_remote_url(&url) {
            return Ok(remote.to_string());
        }
    }
    anyhow::bail!(
        "refusing source build: checkout has no remote for canonical fork {CANONICAL_FORK_REPOSITORY}"
    )
}

fn canonical_release_ref(repo_dir: &Path, remote: &str) -> Result<String> {
    let remote_ref = format!("refs/remotes/{remote}/{CANONICAL_FORK_RELEASE_BRANCH}");
    if git_ref_exists(repo_dir, &remote_ref)? {
        return Ok(remote_ref);
    }

    anyhow::bail!(
        "refusing source build: fetched canonical fork release ref {remote_ref} is not present"
    )
}

/// Validate that a source checkout is a canonical fork release build target.
///
/// The checkout may be exactly at the canonical release branch, or it may be
/// an explicitly admitted merge descendant. In the latter case the caller
/// must provide the upstream commit that was fetched as intake, and the local
/// graph must prove both canonical ancestry and a merge commit. The authority
/// string used in the eventual receipt is intentionally not an input here.
pub fn canonical_source_provenance(
    repo_dir: &Path,
    expected_commit: &str,
    intake_commit: Option<&str>,
) -> Result<ForkBuildSource> {
    if !repo_dir.join(".git").exists() {
        anyhow::bail!(
            "refusing source build: {} is not a git checkout",
            repo_dir.display()
        );
    }
    if !is_hex_commit(expected_commit) {
        anyhow::bail!("refusing source build: expected commit is not a valid git SHA");
    }

    let remote = canonical_fork_remote(repo_dir)?;
    let release_ref = canonical_release_ref(repo_dir, &remote)?;
    let release_commit = git_output(repo_dir, &["rev-parse", &release_ref])?;
    let head = git_output(repo_dir, &["rev-parse", "HEAD"])?;

    if !commit_matches(expected_commit, &head) {
        anyhow::bail!(
            "refusing source build: checkout HEAD {head} does not match expected commit {expected_commit}"
        );
    }
    if !git_is_ancestor(repo_dir, &release_commit, &head)? {
        anyhow::bail!(
            "refusing source build: HEAD {head} is not a descendant of canonical release branch {release_commit}"
        );
    }

    let intake_commit = if head == release_commit {
        if intake_commit.is_some() {
            anyhow::bail!(
                "refusing source build: canonical release branch head cannot claim an upstream intake merge"
            );
        }
        None
    } else {
        let intake_commit = intake_commit
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "refusing source build: non-branch HEAD requires an explicitly admitted upstream merge"
                )
            })?;
        if !is_hex_commit(intake_commit) {
            anyhow::bail!("refusing source build: intake commit is not a valid git SHA");
        }
        let intake_commit = git_output(
            repo_dir,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                "--end-of-options",
                &format!("{intake_commit}^{{commit}}"),
            ],
        )?;
        if !git_is_ancestor(repo_dir, &intake_commit, &head)? {
            anyhow::bail!(
                "refusing source build: intake commit {intake_commit} is not an ancestor of canonical descendant {head}"
            );
        }
        if git_is_ancestor(repo_dir, &intake_commit, &release_commit)? {
            anyhow::bail!(
                "refusing source build: intake commit {intake_commit} is already in the canonical release history"
            );
        }

        let ancestry_range = format!("{release_commit}..{head}");
        let merge_commits = git_output(
            repo_dir,
            &["rev-list", "--merges", "--ancestry-path", &ancestry_range],
        )?;
        if merge_commits.is_empty() {
            anyhow::bail!(
                "refusing source build: canonical descendant {head} has no verified merge commit"
            );
        }
        Some(intake_commit)
    };

    let dirty = git_output(repo_dir, &["status", "--porcelain"])?;
    if !dirty.is_empty() {
        anyhow::bail!(
            "refusing source build: canonical checkout {} has uncommitted changes",
            repo_dir.display()
        );
    }

    Ok(ForkBuildSource {
        repository: CANONICAL_FORK_REPOSITORY.to_string(),
        branch: CANONICAL_FORK_RELEASE_BRANCH.to_string(),
        commit: head,
        intake_commit,
    })
}

fn validate_recorded_source(source: &ForkBuildSource, git_hash: &str) -> Result<()> {
    if source.repository != CANONICAL_FORK_REPOSITORY {
        anyhow::bail!(
            "fork build source repository {:?} is not the canonical fork {CANONICAL_FORK_REPOSITORY}",
            source.repository
        );
    }
    if source.branch != CANONICAL_FORK_RELEASE_BRANCH {
        anyhow::bail!(
            "fork build source branch {:?} is not the canonical release branch {CANONICAL_FORK_RELEASE_BRANCH}",
            source.branch
        );
    }
    if !is_hex_commit(&source.commit) || !commit_matches(git_hash, &source.commit) {
        anyhow::bail!(
            "fork build source commit {} does not match binary git hash {}",
            source.commit,
            git_hash
        );
    }
    if let Some(intake_commit) = source.intake_commit.as_deref()
        && (!is_hex_commit(intake_commit) || intake_commit == source.commit)
    {
        anyhow::bail!("fork build source has an invalid upstream intake commit");
    }
    Ok(())
}

fn release_version(value: &str) -> Option<(u64, u64, u64)> {
    let value = value.trim().trim_start_matches('v');
    let value = value.split([' ', '(', '-']).next()?;
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn binary_sha256(binary: &Path) -> Result<String> {
    let payload = resolve_binary_payload(binary);
    let bytes = std::fs::read(&payload).map_err(|error| {
        anyhow::anyhow!(
            "could not read build payload {}: {error}",
            payload.display()
        )
    })?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

fn source_fingerprint_for_binary(binary: &Path) -> Result<Option<String>> {
    let path = binary_source_metadata_path(binary);
    if !path.exists() {
        return Ok(None);
    }
    let metadata: DevBinarySourceMetadata = storage::read_json(&path)?;
    Ok(Some(metadata.source_fingerprint))
}

fn manifest_for_report(
    binary: &Path,
    report: &BinaryVersionReport,
    manifest_trust: ManifestTrust,
    manifest_root: &Path,
) -> Result<(ForkCapabilityManifest, bool)> {
    if let Some(manifest) = read_manifest_for_binary(binary)? {
        let legacy = if manifest.is_legacy() {
            if manifest.schema_version != LEGACY_MANIFEST_SCHEMA_VERSION {
                anyhow::bail!(
                    "legacy capability manifest {} uses unsupported schema {}; only the host-generated schema {} migration manifest is trusted",
                    binary.display(),
                    manifest.schema_version,
                    LEGACY_MANIFEST_SCHEMA_VERSION
                );
            }
            if manifest_trust != ManifestTrust::TrustedLegacyMigration {
                anyhow::bail!(
                    "legacy capability manifest {} requires an explicit host-owned migration context",
                    binary.display()
                );
            }
            let expected = fork_governance::legacy_manifest(
                &report.capabilities,
                report.git_hash.as_deref().unwrap_or("legacy"),
            )?;
            if manifest != expected {
                anyhow::bail!(
                    "legacy capability manifest {} is not the host-generated migration manifest",
                    binary.display()
                );
            }
            true
        } else if manifest.schema_version == LEGACY_MANIFEST_SCHEMA_VERSION {
            anyhow::bail!(
                "schema {} capability manifest {} requires the host-generated legacy migration manifest and an explicit host-owned migration context",
                LEGACY_MANIFEST_SCHEMA_VERSION,
                binary.display()
            );
        } else {
            manifest_trust == ManifestTrust::TrustedLegacyMigration
                && manifest.is_pre_baseline_shape()
        };
        if !legacy {
            manifest.validate_baseline(&builtin_manifest()?)?;
            let version = report.manifest_version.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "post-governance binary {} omitted its capability manifest version",
                    binary.display()
                )
            })?;
            if version != manifest.manifest_version {
                anyhow::bail!(
                    "binary manifest schema/version {} does not match installed manifest {}",
                    version,
                    manifest.manifest_version
                );
            }
            let digest = report.manifest_sha256.as_deref().ok_or_else(|| {
                anyhow::anyhow!(
                    "post-governance binary {} omitted its capability manifest digest",
                    binary.display()
                )
            })?;
            manifest.validate_freshness(digest)?;
            manifest
                .validate_generated_commit(manifest_root)
                .with_context(|| {
                    format!(
                        "post-governance binary {} has an unverifiable generated manifest commit",
                        binary.display()
                    )
                })?;
        }
        return Ok((manifest, legacy));
    }

    // A post-governance binary carries the generated manifest digest in its
    // version report. It is safe to reconstruct the checked-in generated
    // manifest only when that digest matches the trusted compile-time source.
    // The immutable sidecar is written before the admission receipt.
    if report.manifest_version.is_some() || report.manifest_sha256.is_some() {
        let generated = builtin_manifest()?;
        let generated_digest = generated.digest()?;
        if report.manifest_version.as_deref() != Some(generated.manifest_version.as_str())
            || report.manifest_sha256.as_deref() != Some(generated_digest.as_str())
        {
            anyhow::bail!(
                "post-governance build {} is missing a trusted capability manifest sidecar",
                binary.display()
            );
        }
        generated.validate_generated_commit(manifest_root)?;
        return Ok((generated, false));
    }

    if manifest_trust != ManifestTrust::TrustedLegacyMigration {
        anyhow::bail!(
            "build {} has no modern capability manifest identity; legacy migration requires an explicit host-owned context",
            binary.display()
        );
    }

    // Old installed binaries are migrated only after a host-owned path has
    // proved their provenance. The returned manifest is synthesized here,
    // rather than trusting an absence of fields in the candidate report.
    Ok((
        fork_governance::legacy_manifest(
            &report.capabilities,
            report.git_hash.as_deref().unwrap_or("legacy"),
        )?,
        true,
    ))
}

fn validate_manifest_report_identity(
    binary: &Path,
    report: &BinaryVersionReport,
    manifest: &ForkCapabilityManifest,
    legacy: bool,
) -> Result<()> {
    if legacy {
        return Ok(());
    }

    let version = report.manifest_version.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "admitted build {} omitted its capability manifest version",
            binary.display()
        )
    })?;
    if version != manifest.manifest_version {
        anyhow::bail!(
            "admitted build {} manifest version changed: receipt {}, report {}",
            binary.display(),
            manifest.manifest_version,
            version
        );
    }

    let digest = report.manifest_sha256.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "admitted build {} omitted its capability manifest digest",
            binary.display()
        )
    })?;
    manifest.validate_freshness(digest).with_context(|| {
        format!(
            "admitted build {} capability manifest identity is invalid",
            binary.display()
        )
    })
}

fn write_immutable_source_metadata(binary: &Path, source: &SourceState) -> Result<()> {
    let path = binary_source_metadata_path(binary);
    let metadata = DevBinarySourceMetadata::from(source);
    if path.exists() {
        let existing: DevBinarySourceMetadata = storage::read_json(&path)?;
        if existing != metadata {
            anyhow::bail!(
                "refusing to replace immutable source identity for {}",
                binary.display()
            );
        }
        return Ok(());
    }
    storage::write_json(&path, &metadata)
}

fn capability_loss<'a>(candidate: &'a [String], predecessor: &'a [String]) -> Vec<&'a String> {
    predecessor
        .iter()
        .filter(|capability| !candidate.iter().any(|value| value == *capability))
        .collect()
}

fn validate_transition_metadata(
    version: &str,
    authority: &str,
    git_hash: &str,
    capabilities: &[String],
    predecessor: Option<&ForkBuildAdmission>,
    allowed_retirements: &[String],
) -> Result<()> {
    if !canonical_authority(authority) {
        anyhow::bail!(
            "build authority {authority:?} is not the configured canonical fork {CANONICAL_FORK_REPOSITORY}"
        );
    }
    if git_hash.trim().is_empty() || git_hash.trim() == "unknown" {
        anyhow::bail!("refusing to admit fork build {version}: binary omitted git_hash");
    }

    if capabilities.is_empty() {
        anyhow::bail!(
            "refusing to admit fork build {version}: binary omitted the generated capability manifest"
        );
    }

    if let Some(predecessor) = predecessor {
        let lost = capability_loss(capabilities, &predecessor.capabilities)
            .into_iter()
            .filter(|capability| {
                !allowed_retirements
                    .iter()
                    .any(|retired| retired == *capability)
            })
            .collect::<Vec<_>>();
        if !lost.is_empty() {
            let lost = lost
                .into_iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "refusing to admit fork build {version}: capabilities lost from predecessor {}: {lost}",
                predecessor.version
            );
        }

        if let (Some(candidate), Some(previous)) = (
            release_version(version),
            release_version(&predecessor.version),
        ) && candidate <= previous
        {
            anyhow::bail!(
                "refusing to admit fork build {version}: release line does not advance admitted predecessor {}",
                predecessor.version
            );
        }
    }

    Ok(())
}

fn build_admission_path(version: &str) -> Result<PathBuf> {
    let binary = version_binary_path(version)?;
    let directory = binary
        .parent()
        .ok_or_else(|| anyhow::anyhow!("version binary has no parent: {}", binary.display()))?;
    Ok(directory.join("fork-admission.json"))
}

fn admission_manifest_root(explicit: Option<&Path>) -> Result<PathBuf> {
    explicit.map(Path::to_path_buf).or_else(get_repo_dir).ok_or_else(|| {
        anyhow::anyhow!(
            "cannot admit a modern fork build without a canonical repository checkout to verify the generated manifest commit"
        )
    })
}

pub fn read_build_admission(version: &str) -> Result<Option<ForkBuildAdmission>> {
    let path = build_admission_path(version)?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(storage::read_json(&path)?))
}

pub fn admit_installed_fork_build(
    version: &str,
    authority: &str,
    predecessor: Option<&str>,
) -> Result<ForkBuildAdmission> {
    let manifest_root = admission_manifest_root(None)?;
    admit_installed_fork_build_with_root(version, authority, predecessor, &manifest_root)
}

fn admit_installed_fork_build_with_root(
    version: &str,
    authority: &str,
    predecessor: Option<&str>,
    manifest_root: &Path,
) -> Result<ForkBuildAdmission> {
    admit_installed_fork_build_inner(
        version,
        authority,
        predecessor,
        None,
        false,
        ManifestTrust::Modern,
        manifest_root,
    )
}

/// Admit a binary built from a verified canonical fork checkout.
///
/// Unlike [`admit_installed_fork_build`], this API does not accept an authority
/// string. It derives the authority from the checkout after proving that the
/// binary commit is on the canonical release branch or an explicitly admitted
/// merge descendant. Upstream commits therefore remain intake-only.
pub fn admit_installed_canonical_source_build(
    version: &str,
    repo_dir: &Path,
    expected_commit: &str,
    intake_commit: Option<&str>,
    predecessor: Option<&str>,
) -> Result<ForkBuildAdmission> {
    admit_installed_canonical_source_build_with_manifest_trust(
        version,
        repo_dir,
        expected_commit,
        intake_commit,
        predecessor,
        ManifestTrust::Modern,
    )
}

/// Admit the real historical pre-baseline predecessor through the explicit
/// host-owned migration path. The checkout provenance is still verified before
/// any legacy-shaped manifest is accepted.
pub fn admit_installed_legacy_canonical_source_build(
    version: &str,
    repo_dir: &Path,
    expected_commit: &str,
    intake_commit: Option<&str>,
    predecessor: Option<&str>,
) -> Result<ForkBuildAdmission> {
    admit_installed_canonical_source_build_with_manifest_trust(
        version,
        repo_dir,
        expected_commit,
        intake_commit,
        predecessor,
        ManifestTrust::TrustedLegacyMigration,
    )
}

fn admit_installed_canonical_source_build_with_manifest_trust(
    version: &str,
    repo_dir: &Path,
    expected_commit: &str,
    intake_commit: Option<&str>,
    predecessor: Option<&str>,
    manifest_trust: ManifestTrust,
) -> Result<ForkBuildAdmission> {
    let source = canonical_source_provenance(repo_dir, expected_commit, intake_commit)?;
    let authority = format!("local-fork:{}:{}", source.repository, source.branch);
    let binary = version_binary_path(version)?;
    let report = read_binary_version_report(&binary)?;
    let (manifest, legacy) = manifest_for_report(&binary, &report, manifest_trust, repo_dir)?;
    if !legacy {
        manifest.validate_source_paths(repo_dir)?;
    }
    admit_installed_fork_build_inner(
        version,
        &authority,
        predecessor,
        Some(source),
        false,
        manifest_trust,
        repo_dir,
    )
}

fn admit_installed_fork_build_inner(
    version: &str,
    authority: &str,
    predecessor: Option<&str>,
    source: Option<ForkBuildSource>,
    initial_reseed: bool,
    manifest_trust: ManifestTrust,
    manifest_root: &Path,
) -> Result<ForkBuildAdmission> {
    if initial_reseed && (predecessor.is_some() || any_fork_admission_receipt_exists()?) {
        anyhow::bail!("refusing initial fork reseed after an admitted release line exists");
    }
    if source.is_none() && source_intake_authority(authority) {
        anyhow::bail!("refusing source-intake admission without verified canonical fork ancestry");
    }
    let binary = version_binary_path(version)?;
    let report = read_binary_version_report(&binary)?;
    if let Some(expected) = release_version(version) {
        let reported = report
            .version
            .as_deref()
            .and_then(release_version)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "refusing to admit fork build {version}: binary omitted a parseable release version"
                )
            })?;
        if reported != expected {
            anyhow::bail!(
                "refusing to admit fork build {version}: binary reports version {:?}",
                report.version
            );
        }
    }
    let git_hash = report.git_hash.clone().unwrap_or_default();
    let (mut candidate_manifest, candidate_legacy) =
        manifest_for_report(&binary, &report, manifest_trust, manifest_root)?;
    if let Some(source) = source.as_ref() {
        validate_recorded_source(source, &git_hash)?;
    }
    let predecessor_admission = predecessor
        .map(|predecessor| require_admitted_fork_build_with_root(predecessor, manifest_root))
        .transpose()
        .map_err(|error| {
            anyhow::anyhow!("build {version} does not extend an admitted fork release: {error}")
        })?;

    let mut predecessor_legacy = false;
    let predecessor_manifest = if let Some(predecessor_admission) = predecessor_admission.as_ref() {
        let predecessor_binary = version_binary_path(&predecessor_admission.version)?;
        let predecessor_report = read_binary_version_report(&predecessor_binary)?;
        let (manifest, legacy) = manifest_for_report(
            &predecessor_binary,
            &predecessor_report,
            predecessor_admission.manifest_trust,
            manifest_root,
        )?;
        predecessor_legacy = legacy;
        Some(manifest)
    } else {
        None
    };

    if candidate_legacy && predecessor_manifest.is_some() && !predecessor_legacy {
        anyhow::bail!(
            "refusing legacy manifest migration for {version}: candidate cannot follow a modern predecessor"
        );
    }

    if let Some(predecessor_manifest) = predecessor_manifest.as_ref()
        && !candidate_legacy
        && !predecessor_legacy
    {
        candidate_manifest.predecessor_manifest_sha256 = Some(predecessor_manifest.digest()?);
        candidate_manifest.sha256 = None;
        ForkCapabilityManifest::validate_transition(
            predecessor_manifest,
            &candidate_manifest,
            version,
        )?;
    }

    // Once a release-line head exists, every new build must name that exact
    // head. This prevents a higher-semver build from silently starting a new
    // line from an upstream or unrelated admitted receipt.
    if !initial_reseed
        && let Some(head) = admitted_release_line_head()?
        && predecessor != Some(head.as_str())
    {
        anyhow::bail!(
            "refusing to admit fork build {version}: predecessor {:?} is not the admitted release-line head {head}",
            predecessor
        );
    }

    validate_transition_metadata(
        version,
        authority,
        &git_hash,
        &report.capabilities,
        predecessor_admission.as_ref(),
        &candidate_manifest
            .retirements
            .iter()
            .map(|record| record.capability_id.clone())
            .collect::<Vec<_>>(),
    )?;

    write_immutable_manifest(&binary, &candidate_manifest)?;

    let binary_sha256 = binary_sha256(&binary)?;
    let source_fingerprint = source_fingerprint_for_binary(&binary)?;

    // Version directories and their admission receipts are immutable. A
    // repeated install is idempotent only when every identity field matches.
    if let Some(existing) = read_build_admission(version)? {
        require_admitted_fork_build_with_root(version, manifest_root)?;
        if existing.git_hash == git_hash
            && existing.authority == authority
            && existing.binary_sha256 == binary_sha256
            && existing.predecessor.as_deref() == predecessor
            && existing.source_fingerprint == source_fingerprint
            && existing.source == source
            && existing.manifest_trust
                == if candidate_legacy {
                    ManifestTrust::TrustedLegacyMigration
                } else {
                    ManifestTrust::Modern
                }
            && existing.capabilities == report.capabilities
        {
            return Ok(existing);
        }
        anyhow::bail!(
            "refusing to repin immutable fork build {version}: admission identity differs"
        );
    }

    let admission = ForkBuildAdmission {
        version: version.to_string(),
        git_hash,
        authority: authority.to_string(),
        binary_sha256,
        predecessor: predecessor.map(str::to_string),
        source_fingerprint,
        source,
        manifest_trust: if candidate_legacy {
            ManifestTrust::TrustedLegacyMigration
        } else {
            ManifestTrust::Modern
        },
        capabilities: report.capabilities,
        admitted_at: Utc::now(),
    };
    storage::write_json(&build_admission_path(version)?, &admission)?;
    Ok(admission)
}

pub fn require_admitted_fork_build(version: &str) -> Result<ForkBuildAdmission> {
    let manifest_root = admission_manifest_root(None)?;
    require_admitted_fork_build_with_root(version, &manifest_root)
}

fn require_admitted_fork_build_with_root(
    version: &str,
    manifest_root: &Path,
) -> Result<ForkBuildAdmission> {
    require_admitted_fork_build_inner(version, &mut HashSet::new(), manifest_root)
}

fn require_admitted_fork_build_inner(
    version: &str,
    seen: &mut HashSet<String>,
    manifest_root: &Path,
) -> Result<ForkBuildAdmission> {
    if !seen.insert(version.to_string()) {
        anyhow::bail!("fork admission predecessor cycle includes {version}");
    }
    let admission = read_build_admission(version)?
        .ok_or_else(|| anyhow::anyhow!("build {version} has no fork admission receipt"))?;
    if admission.version != version {
        anyhow::bail!(
            "fork admission version mismatch: receipt says {}, requested {version}",
            admission.version
        );
    }
    if !canonical_authority(&admission.authority) {
        anyhow::bail!(
            "fork admission for {version} has untrusted authority {:?}",
            admission.authority
        );
    }
    if source_intake_authority(&admission.authority) {
        anyhow::bail!("fork admission for {version} uses an unverified source-intake authority");
    }
    if admission.binary_sha256.trim().is_empty() {
        anyhow::bail!("fork admission for {version} omitted immutable binary identity");
    }
    let binary = version_binary_path(version)?;
    if !binary.exists() {
        anyhow::bail!("admitted fork build {version} is missing its binary");
    }
    let actual_sha256 = binary_sha256(&binary)?;
    if actual_sha256 != admission.binary_sha256 {
        anyhow::bail!(
            "admitted fork build {version} binary identity changed: receipt {}, actual {}",
            admission.binary_sha256,
            actual_sha256
        );
    }
    if source_fingerprint_for_binary(&binary)? != admission.source_fingerprint {
        anyhow::bail!("admitted fork build {version} source identity changed");
    }

    let report = read_binary_version_report(&binary)?;
    let manifest_sidecar_exists = manifest_path_for_binary(&binary).exists();
    let (manifest, legacy_manifest) =
        manifest_for_report(&binary, &report, admission.manifest_trust, manifest_root)?;
    if !manifest_sidecar_exists && !legacy_manifest {
        anyhow::bail!(
            "admitted fork build {version} is missing its post-governance capability manifest sidecar"
        );
    }
    validate_manifest_report_identity(&binary, &report, &manifest, legacy_manifest)?;
    let manifest_capabilities = manifest
        .capabilities
        .iter()
        .map(|capability| capability.id.clone())
        .collect::<Vec<_>>();
    if manifest_capabilities != report.capabilities {
        anyhow::bail!(
            "admitted fork build {version} capability manifest changed: manifest {:?}, binary {:?}",
            manifest_capabilities,
            report.capabilities
        );
    }
    if let Some(expected) = release_version(version) {
        let reported = report
            .version
            .as_deref()
            .and_then(release_version)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "admitted fork build {version} no longer reports a parseable release version"
                )
            })?;
        if reported != expected {
            anyhow::bail!(
                "admitted fork build {version} reports a different version {:?}",
                report.version
            );
        }
    }
    if report.git_hash.as_deref().unwrap_or_default() != admission.git_hash {
        anyhow::bail!(
            "admitted fork build {version} git identity changed: receipt {}, binary {:?}",
            admission.git_hash,
            report.git_hash
        );
    }
    if let Some(source) = admission.source.as_ref() {
        validate_recorded_source(source, &admission.git_hash)?;
    }
    if report.capabilities != admission.capabilities {
        anyhow::bail!(
            "admitted fork build {version} capability manifest changed: receipt {:?}, binary {:?}",
            admission.capabilities,
            report.capabilities
        );
    }

    if let Some(predecessor_version) = admission.predecessor.as_deref()
        && !legacy_manifest
    {
        let predecessor_binary = version_binary_path(predecessor_version)?;
        let predecessor_report = read_binary_version_report(&predecessor_binary)?;
        let predecessor_trust = read_build_admission(predecessor_version)?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "admitted predecessor {predecessor_version} has no fork admission receipt"
                )
            })?
            .manifest_trust;
        let (predecessor_manifest, predecessor_legacy) = manifest_for_report(
            &predecessor_binary,
            &predecessor_report,
            predecessor_trust,
            manifest_root,
        )?;
        if !predecessor_legacy {
            ForkCapabilityManifest::validate_transition(&predecessor_manifest, &manifest, version)?;
        }
    }

    let allowed_retirements = if legacy_manifest {
        Vec::new()
    } else {
        manifest
            .retirements
            .iter()
            .map(|record| record.capability_id.clone())
            .collect::<Vec<_>>()
    };
    if let Some(predecessor) = admission.predecessor.as_deref() {
        let predecessor = require_admitted_fork_build_inner(predecessor, seen, manifest_root)?;
        validate_transition_metadata(
            version,
            &admission.authority,
            &admission.git_hash,
            &admission.capabilities,
            Some(&predecessor),
            &allowed_retirements,
        )?;
    } else {
        validate_transition_metadata(
            version,
            &admission.authority,
            &admission.git_hash,
            &admission.capabilities,
            None,
            &allowed_retirements,
        )?;
    }
    Ok(admission)
}

pub fn admitted_predecessor(version: Option<String>) -> Result<Option<String>> {
    let Some(version) = version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
    else {
        return Ok(None);
    };
    require_admitted_fork_build(&version).map_err(|error| {
        anyhow::anyhow!(
            "existing release-line head {version} is not an admitted fork build: {error}"
        )
    })?;
    Ok(Some(version))
}

pub fn admitted_release_line_head() -> Result<Option<String>> {
    let mut rejected = Vec::new();
    for version in [
        read_current_version()?,
        read_shared_server_version()?,
        read_stable_version()?,
    ]
    .into_iter()
    .flatten()
    {
        let version = version.trim();
        if !version.is_empty() {
            match require_admitted_fork_build(version) {
                Ok(_) => return Ok(Some(version.to_string())),
                Err(error) => rejected.push(format!("{version}: {error:#}")),
            }
        }
    }
    if !rejected.is_empty() {
        anyhow::bail!(
            "configured release channels contain no admitted fork build: {}",
            rejected.join(" | ")
        );
    }
    Ok(None)
}

fn any_fork_admission_receipt_exists() -> Result<bool> {
    let versions = builds_dir()?.join("versions");
    if !versions.exists() {
        return Ok(false);
    }
    for entry in std::fs::read_dir(versions)? {
        if entry?.path().join("fork-admission.json").exists() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Adopt the single pre-governance fork build already pinned on this machine.
///
/// This is intentionally a one-time migration, not a general caller-selected
/// admission path. It runs only before any admission receipt exists, requires
/// exactly one configured installed version to prove canonical-fork ancestry
/// and required capabilities against the current clean checkout. This permits
/// recovery when an external updater has repinned some channels to an
/// ineligible upstream release, while zero or multiple eligible builds remain
/// fail-closed.
pub fn bootstrap_legacy_fork_build(repo_dir: &Path) -> Result<Option<ForkBuildAdmission>> {
    if any_fork_admission_receipt_exists()? {
        return Ok(None);
    }

    let configured = [
        read_current_version()?,
        read_shared_server_version()?,
        read_stable_version()?,
    ]
    .into_iter()
    .flatten()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
    .collect::<HashSet<_>>();

    if configured.is_empty() {
        return Ok(None);
    }
    canonical_fork_remote(repo_dir)?;
    let source = current_source_state(repo_dir)?;
    if source.dirty {
        anyhow::bail!("refusing legacy fork bootstrap from a dirty source checkout");
    }

    let authority = format!("local-fork:{CANONICAL_FORK_REPOSITORY}:legacy-bootstrap");
    let mut configured = configured.into_iter().collect::<Vec<_>>();
    configured.sort();
    let mut eligible = Vec::new();
    let mut rejected = Vec::new();
    for version in configured {
        let candidate = (|| -> Result<_> {
            let binary = version_binary_path(&version)?;
            if !binary.exists() {
                anyhow::bail!("configured binary {} is missing", binary.display());
            }
            let report = read_binary_version_report(&binary)?;
            let reported_hash = report.git_hash.clone().unwrap_or_default();
            if !is_hex_commit(&reported_hash) {
                anyhow::bail!("binary omitted a valid git hash");
            }
            let commit = git_output(
                repo_dir,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    "--end-of-options",
                    &format!("{reported_hash}^{{commit}}"),
                ],
            )?;
            if !git_is_ancestor(repo_dir, &commit, &source.full_hash)? {
                anyhow::bail!(
                    "binary commit {commit} is not an ancestor of maintained fork head {}",
                    source.full_hash
                );
            }
            validate_transition_metadata(
                &version,
                &authority,
                &reported_hash,
                &report.capabilities,
                None,
                &[],
            )?;
            Ok((version.clone(), binary, report, reported_hash))
        })();
        match candidate {
            Ok(candidate) => eligible.push(candidate),
            Err(error) => rejected.push(format!("{version}: {error:#}")),
        }
    }
    if eligible.is_empty() {
        return Ok(None);
    }
    if eligible.len() != 1 {
        anyhow::bail!(
            "refusing legacy fork bootstrap: expected exactly one eligible configured build, found {}; rejected: {}",
            eligible.len(),
            rejected.join(" | ")
        );
    }
    let (version, binary, report, reported_hash) =
        eligible.pop().expect("one eligible configured build");
    let admission = ForkBuildAdmission {
        version: version.clone(),
        git_hash: reported_hash,
        authority,
        binary_sha256: binary_sha256(&binary)?,
        predecessor: None,
        source_fingerprint: source_fingerprint_for_binary(&binary)?,
        source: None,
        manifest_trust: ManifestTrust::TrustedLegacyMigration,
        capabilities: report.capabilities,
        admitted_at: Utc::now(),
    };
    storage::write_json(&build_admission_path(&version)?, &admission)?;
    require_admitted_fork_build_with_root(&version, repo_dir)?;
    Ok(Some(admission))
}

fn admit_initial_local_fork_build(
    repo_dir: &Path,
    source: &SourceState,
) -> Result<ForkBuildAdmission> {
    if source.dirty {
        anyhow::bail!("refusing initial fork reseed from a dirty source checkout");
    }
    let remote = canonical_fork_remote(repo_dir)?;
    let release_ref = canonical_release_ref(repo_dir, &remote)?;
    let release_commit = git_output(repo_dir, &["rev-parse", &release_ref])?;
    if !git_is_ancestor(repo_dir, &release_commit, &source.full_hash)? {
        anyhow::bail!(
            "refusing initial fork reseed: source {} does not descend from canonical fork release {release_commit}",
            source.full_hash
        );
    }
    admit_installed_fork_build_inner(
        &source.version_label,
        &format!("local-fork:{CANONICAL_FORK_REPOSITORY}:initial-reseed"),
        None,
        None,
        true,
        ManifestTrust::Modern,
        repo_dir,
    )
}

/// Manifest tracking build versions and their status
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildManifest {
    /// Current stable build hash (known good)
    pub stable: Option<String>,
    /// Current canary build hash (being tested)
    pub canary: Option<String>,
    /// Session ID testing the canary build
    pub canary_session: Option<String>,
    /// Status of canary testing
    pub canary_status: Option<CanaryStatus>,
    /// History of recent builds
    #[serde(default)]
    pub history: Vec<BuildInfo>,
    /// Last crash information (if canary crashed)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_crash: Option<CrashInfo>,
    /// Pending activation being validated across reload/resume.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_activation: Option<PendingActivation>,
}

impl BuildManifest {
    /// Load manifest from disk
    pub fn load() -> Result<Self> {
        let path = manifest_path()?;
        if path.exists() {
            storage::read_json(&path)
        } else {
            Ok(Self::default())
        }
    }

    /// Save manifest to disk
    pub fn save(&self) -> Result<()> {
        let path = manifest_path()?;
        storage::write_json(&path, self)
    }

    /// Check if we should use stable or canary for a given session
    pub fn binary_for_session(&self, session_id: &str) -> BinaryChoice {
        // If this session is the canary tester, use canary
        if let Some(ref canary_session) = self.canary_session
            && canary_session == session_id
            && let Some(ref canary) = self.canary
        {
            return BinaryChoice::Canary(canary.clone());
        }
        // Otherwise use stable
        if let Some(ref stable) = self.stable {
            BinaryChoice::Stable(stable.clone())
        } else {
            BinaryChoice::Current
        }
    }

    /// Start canary testing for a session
    pub fn start_canary(&mut self, hash: &str, session_id: &str) -> Result<()> {
        self.canary = Some(hash.to_string());
        self.canary_session = Some(session_id.to_string());
        self.canary_status = Some(CanaryStatus::Testing);
        self.save()
    }

    /// Mark canary as passed
    pub fn mark_canary_passed(&mut self) -> Result<()> {
        self.canary_status = Some(CanaryStatus::Passed);
        self.save()
    }

    /// Mark canary as failed
    pub fn mark_canary_failed(&mut self) -> Result<()> {
        self.canary_status = Some(CanaryStatus::Failed);
        self.save()
    }

    /// Record a crash
    pub fn record_crash(
        &mut self,
        hash: &str,
        exit_code: i32,
        stderr: &str,
        diff: Option<String>,
    ) -> Result<()> {
        self.last_crash = Some(CrashInfo {
            build_hash: hash.to_string(),
            exit_code,
            stderr: stderr.chars().take(4096).collect(), // Truncate
            crashed_at: Utc::now(),
            diff,
        });
        self.canary_status = Some(CanaryStatus::Failed);
        self.save()
    }

    /// Clear crash info after it's been handled
    pub fn clear_crash(&mut self) -> Result<()> {
        self.last_crash = None;
        self.save()
    }

    pub fn set_pending_activation(&mut self, activation: PendingActivation) -> Result<()> {
        self.pending_activation = Some(activation);
        self.save()
    }

    pub fn clear_pending_activation(&mut self) -> Result<()> {
        self.pending_activation = None;
        self.save()
    }

    /// Add build to history
    pub fn add_to_history(&mut self, info: BuildInfo) -> Result<()> {
        // Keep last 20 builds
        self.history.insert(0, info);
        self.history.truncate(20);
        self.save()
    }
}

pub fn complete_pending_activation_for_session(session_id: &str) -> Result<Option<String>> {
    let mut manifest = BuildManifest::load()?;
    let Some(pending) = manifest.pending_activation.clone() else {
        return Ok(None);
    };
    if pending.session_id != session_id {
        return Ok(None);
    }

    require_admitted_fork_build(&pending.new_version).map_err(|error| {
        anyhow::anyhow!(
            "cannot mark pending activation {} as passed: build is not admitted: {error}",
            pending.new_version
        )
    })?;
    manifest.canary = Some(pending.new_version.clone());
    manifest.canary_session = Some(session_id.to_string());
    manifest.canary_status = Some(CanaryStatus::Passed);
    manifest.pending_activation = None;
    manifest.last_crash = None;
    manifest.save()?;
    Ok(Some(pending.new_version))
}

pub fn rollback_pending_activation_for_session(session_id: &str) -> Result<Option<String>> {
    let mut manifest = BuildManifest::load()?;
    let Some(pending) = manifest.pending_activation.clone() else {
        return Ok(None);
    };
    if pending.session_id != session_id {
        return Ok(None);
    }

    let mut rollback_error = None;
    for (channel, previous) in [
        ("current", pending.previous_current_version.as_deref()),
        (
            "shared-server",
            pending.previous_shared_server_version.as_deref(),
        ),
    ] {
        if let Some(previous) = previous
            && let Err(error) = require_admitted_fork_build(previous)
        {
            rollback_error = Some(anyhow::anyhow!(
                "{channel} rollback target {previous} is not an admitted fork build: {error}"
            ));
            break;
        }
    }

    // Validate every rollback target before changing either channel. This keeps
    // a malformed secondary target from leaving current restored while the
    // shared-server channel remains on the failed activation.
    if let Some(error) = rollback_error.as_ref() {
        manifest.canary_status = Some(CanaryStatus::Failed);
        manifest.pending_activation = None;
        manifest.save()?;
        anyhow::bail!(
            "rollback of pending activation {} refused: {error}",
            pending.new_version
        );
    }

    if let Some(previous) = pending.previous_current_version.as_deref() {
        match update_current_to_admitted_fork_build(previous) {
            Ok(_) => {
                if let Err(error) = update_launcher_symlink_to_current() {
                    rollback_error = Some(error);
                }
            }
            Err(error) => rollback_error = Some(error),
        }
    }
    if let Some(previous) = pending.previous_shared_server_version.as_deref()
        && let Err(error) = update_shared_server_to_admitted_fork_build(previous)
        && rollback_error.is_none()
    {
        rollback_error = Some(error);
    }
    manifest.canary_status = Some(CanaryStatus::Failed);
    manifest.pending_activation = None;
    manifest.save()?;
    if let Some(error) = rollback_error {
        anyhow::bail!(
            "rollback of pending activation {} refused because the target was not an admitted fork build: {error}",
            pending.new_version
        );
    }
    Ok(Some(pending.new_version))
}

/// Install a binary at a specific immutable version path.
pub fn install_binary_at_version(source: &std::path::Path, version: &str) -> Result<PathBuf> {
    if !source.exists() {
        anyhow::bail!("Binary not found at {:?}", source);
    }

    let dest_dir = builds_dir()?.join("versions").join(version);
    storage::ensure_dir(&dest_dir)?;

    let dest = dest_dir.join(binary_name());

    // Version paths are immutable. Re-installing the exact same bytes is
    // idempotent, but replacing a version with a different source would let a
    // stale admission receipt authorize a different build.
    if dest.exists() {
        let existing_sha256 = binary_sha256(&dest)?;
        let source_sha256 = binary_sha256(source)?;
        if existing_sha256 == source_sha256 {
            return Ok(dest);
        }
        anyhow::bail!(
            "refusing to replace immutable build {} with different binary identity",
            version
        );
    }

    // Prefer hard link (instant, zero I/O) over copy (71MB+ binary).
    // Falls back to copy if hard link fails (e.g. cross-filesystem).
    if std::fs::hard_link(source, &dest).is_err() {
        std::fs::copy(source, &dest)?;
    }
    crate::platform_support::set_permissions_executable(&dest)?;

    Ok(dest)
}

fn binary_source_metadata_path(binary: &Path) -> PathBuf {
    let file_name = binary
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| binary_stem().to_string());
    binary.with_file_name(format!("{file_name}.source.json"))
}

pub fn write_dev_binary_source_metadata(binary: &Path, source: &SourceState) -> Result<PathBuf> {
    let path = binary_source_metadata_path(binary);
    storage::write_json(&path, &DevBinarySourceMetadata::from(source))?;
    Ok(path)
}

pub fn write_current_dev_binary_source_metadata(
    repo_dir: &Path,
    source: &SourceState,
) -> Result<PathBuf> {
    let binary = find_dev_binary(repo_dir)
        .ok_or_else(|| anyhow::anyhow!("Binary not found in target/selfdev or target/release"))?;
    write_dev_binary_source_metadata(&binary, source)
}

fn read_binary_version_report(binary: &Path) -> Result<BinaryVersionReport> {
    let output = Command::new(binary)
        .args(["version", "--json"])
        .env("JCODE_NON_INTERACTIVE", "1")
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "Binary smoke test failed for {} with exit code {:?}: {}",
            binary.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let report: BinaryVersionReport = serde_json::from_slice(&output.stdout).map_err(|err| {
        anyhow::anyhow!(
            "Binary smoke test for {} returned invalid JSON: {}",
            binary.display(),
            err
        )
    })?;
    validate_runtime_smoke_report(binary, &report)?;
    Ok(report)
}

fn validate_runtime_smoke_report(binary: &Path, report: &BinaryVersionReport) -> Result<()> {
    if report
        .version
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        anyhow::bail!(
            "Binary smoke test for {} returned JSON without a version field",
            binary.display()
        );
    }
    Ok(())
}

pub fn smoke_test_binary(binary: &Path) -> Result<()> {
    read_binary_version_report(binary)?;
    Ok(())
}

fn validate_binary_version_matches_source_report(
    report: &BinaryVersionReport,
    binary: &Path,
    source: &SourceState,
) -> Result<()> {
    let git_hash = report.git_hash.as_deref().unwrap_or_default();
    if git_hash.is_empty() {
        anyhow::bail!(
            "Binary {} version report did not include git_hash; rebuild before publishing {}",
            binary.display(),
            source.version_label
        );
    }
    if git_hash != source.short_hash {
        anyhow::bail!(
            "Refusing to publish {} as {}: binary was built from git hash {}, but source state is {}",
            binary.display(),
            source.version_label,
            git_hash,
            source.short_hash
        );
    }
    Ok(())
}

fn dirty_status_paths(repo_dir: &Path) -> Result<Vec<(PathBuf, bool)>> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(repo_dir)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "git status failed while validating dirty build freshness with status {:?}",
            output.status.code()
        );
    }

    let mut entries = output.stdout.split(|byte| *byte == 0).peekable();
    let mut paths = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.is_empty() || entry.len() < 4 {
            continue;
        }
        let x = entry[0];
        let y = entry[1];
        let path = String::from_utf8_lossy(&entry[3..]).to_string();
        let deleted = x == b'D' || y == b'D';
        paths.push((PathBuf::from(path), deleted));

        if matches!(x, b'R' | b'C') || matches!(y, b'R' | b'C') {
            let _ = entries.next();
        }
    }

    Ok(paths)
}

fn validate_dirty_binary_freshness_without_metadata(
    repo_dir: &Path,
    binary: &Path,
    source: &SourceState,
) -> Result<()> {
    if !source.dirty {
        return Ok(());
    }

    let binary_mtime = std::fs::metadata(binary)
        .and_then(|metadata| metadata.modified())
        .map_err(|err| {
            anyhow::anyhow!(
                "Could not read binary modification time for {}: {}",
                binary.display(),
                err
            )
        })?;
    let dirty_paths = dirty_status_paths(repo_dir)?;
    let mut unverifiable = Vec::new();
    let mut newer_than_binary = Vec::new();

    for (relative, deleted) in dirty_paths {
        if deleted {
            unverifiable.push(relative.display().to_string());
            continue;
        }
        let path = repo_dir.join(&relative);
        let modified = match std::fs::metadata(&path).and_then(|metadata| metadata.modified()) {
            Ok(modified) => modified,
            Err(_) => {
                unverifiable.push(relative.display().to_string());
                continue;
            }
        };
        if modified > binary_mtime {
            newer_than_binary.push(relative.display().to_string());
        }
    }

    if !unverifiable.is_empty() {
        anyhow::bail!(
            "Refusing to publish dirty build {} without source metadata: these changed paths cannot be checked against the binary timestamp: {}",
            source.version_label,
            unverifiable.join(", ")
        );
    }
    if !newer_than_binary.is_empty() {
        anyhow::bail!(
            "Refusing to publish stale dirty build {}: changed paths are newer than {}: {}",
            source.version_label,
            binary.display(),
            newer_than_binary.join(", ")
        );
    }

    Ok(())
}

fn validate_dev_binary_source_metadata(binary: &Path, source: &SourceState) -> Result<bool> {
    let path = binary_source_metadata_path(binary);
    if !path.exists() {
        return Ok(false);
    }

    let metadata: DevBinarySourceMetadata = storage::read_json(&path)?;
    if metadata.source_fingerprint != source.fingerprint
        || metadata.version_label != source.version_label
        || metadata.short_hash != source.short_hash
        || metadata.full_hash != source.full_hash
        || metadata.dirty != source.dirty
    {
        anyhow::bail!(
            "Refusing to publish {} as {}: source metadata at {} was for {} ({})",
            binary.display(),
            source.version_label,
            path.display(),
            metadata.version_label,
            metadata.source_fingerprint
        );
    }
    Ok(true)
}

fn validate_dev_binary_matches_source(
    repo_dir: &Path,
    binary: &Path,
    source: &SourceState,
) -> Result<()> {
    let report = read_binary_version_report(binary)?;
    if report.version.as_deref().unwrap_or_default().is_empty() {
        anyhow::bail!(
            "Binary smoke test for {} returned JSON without a version field",
            binary.display()
        );
    }
    validate_binary_version_matches_source_report(&report, binary, source)?;
    if !validate_dev_binary_source_metadata(binary, source)? {
        validate_dirty_binary_freshness_without_metadata(repo_dir, binary, source)?;
    }
    Ok(())
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokeTestReplyKind {
    Ack,
    Pong,
}

#[cfg(unix)]
fn smoke_test_server_request(
    stream: &mut BufReader<std::os::unix::net::UnixStream>,
    request: &serde_json::Value,
    expected_reply_kind: SmokeTestReplyKind,
    expected_reply_id: u64,
) -> Result<()> {
    let payload = serde_json::to_string(request)? + "\n";
    stream.get_mut().write_all(payload.as_bytes())?;
    stream.get_mut().flush()?;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut line = String::new();
        let bytes = stream.read_line(&mut line)?;
        if bytes == 0 {
            anyhow::bail!(
                "server closed the smoke-test socket before sending {:?} {}",
                expected_reply_kind,
                expected_reply_id
            );
        }
        let value: serde_json::Value = serde_json::from_str(line.trim()).map_err(|err| {
            anyhow::anyhow!("server smoke test returned invalid JSON line: {}", err)
        })?;
        let reply_type = value.get("type").and_then(|t| t.as_str());
        let reply_id = value.get("id").and_then(|id| id.as_u64());
        let kind_matches = match expected_reply_kind {
            SmokeTestReplyKind::Ack => reply_type == Some("ack"),
            SmokeTestReplyKind::Pong => reply_type == Some("pong"),
        };
        if kind_matches && reply_id == Some(expected_reply_id) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            anyhow::bail!(
                "timed out waiting for {:?} {} during server smoke test",
                expected_reply_kind,
                expected_reply_id
            );
        }
    }
}

#[cfg(unix)]
fn smoke_test_server_connect(
    path: &Path,
) -> std::io::Result<BufReader<std::os::unix::net::UnixStream>> {
    let stream = std::os::unix::net::UnixStream::connect(path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    Ok(BufReader::new(stream))
}

#[cfg(unix)]
fn smoke_test_server_protocol(path: &Path, working_dir: &str) -> Result<()> {
    // The server handles an initial Ping on a dedicated lightweight-control
    // connection and closes it after replying, so the subscribed-client probe
    // must use a fresh socket.
    {
        let mut stream = smoke_test_server_connect(path)?;
        smoke_test_server_request(
            &mut stream,
            &serde_json::json!({
                "type": "ping",
                "id": 1
            }),
            SmokeTestReplyKind::Pong,
            1,
        )?;
    }

    let mut stream = smoke_test_server_connect(path)?;
    smoke_test_server_request(
        &mut stream,
        &serde_json::json!({
            "type": "subscribe",
            "id": 2,
            "working_dir": working_dir
        }),
        SmokeTestReplyKind::Ack,
        2,
    )?;
    Ok(())
}

#[cfg(unix)]
pub fn smoke_test_server_binary(binary: &Path) -> Result<()> {
    use std::fs::File;
    use std::process::Stdio;
    use std::thread;

    smoke_test_binary(binary)?;

    let temp = tempfile::tempdir()?;
    let runtime_dir = temp.path().join("runtime");
    storage::ensure_dir(&runtime_dir)?;
    let socket_path = temp.path().join("jcode-smoke.sock");
    let stderr_path = temp.path().join("jcode-smoke.stderr.log");
    let stderr = File::create(&stderr_path)?;

    let mut child = Command::new(binary)
        .arg("serve")
        .arg("--socket")
        .arg(&socket_path)
        .env("JCODE_NON_INTERACTIVE", "1")
        .env("JCODE_RUNTIME_DIR", &runtime_dir)
        .env("JCODE_GATEWAY_ENABLED", "0")
        .env("JCODE_TEMP_SERVER", "1")
        .env("JCODE_SERVER_OWNER_PID", std::process::id().to_string())
        .env("JCODE_TEMP_SERVER_IDLE_SECS", "300")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()?;

    let result = (|| -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = child.try_wait()? {
                let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
                anyhow::bail!(
                    "server smoke test process exited early with status {:?}: {}",
                    status.code(),
                    stderr.trim()
                );
            }

            match smoke_test_server_connect(&socket_path) {
                Ok(_) => {
                    smoke_test_server_protocol(&socket_path, env!("CARGO_MANIFEST_DIR"))?;
                    return Ok(());
                }
                Err(err)
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::NotFound
                            | std::io::ErrorKind::ConnectionRefused
                            | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    if Instant::now() >= deadline {
                        let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
                        anyhow::bail!(
                            "timed out waiting for server smoke test socket {}: {}",
                            socket_path.display(),
                            stderr.trim()
                        );
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(err) => return Err(err.into()),
            }
        }
    })();

    let _ = child.kill();
    let shutdown_deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= shutdown_deadline {
            let _ = child.kill();
            let _ = child.wait();
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }

    result
}

#[cfg(not(unix))]
pub fn smoke_test_server_binary(binary: &Path) -> Result<()> {
    smoke_test_binary(binary)
}

fn update_channel_symlink(channel: &str, version: &str) -> Result<PathBuf> {
    let channel_dir = builds_dir()?.join(channel);
    storage::ensure_dir(&channel_dir)?;

    let link_path = channel_dir.join(binary_name());
    let target = version_binary_path(version)?;
    if !target.exists() {
        anyhow::bail!("Version binary not found at {:?}", target);
    }

    let temp = channel_dir.join(format!(
        ".{}-{}-{}",
        binary_stem(),
        channel,
        std::process::id()
    ));
    crate::platform_support::atomic_symlink_swap(&target, &link_path, &temp)?;

    Ok(link_path)
}

fn channel_marker_path(channel: &str) -> Result<PathBuf> {
    match channel {
        "stable" => stable_version_file(),
        "current" => current_version_file(),
        "shared-server" => shared_server_version_file(),
        _ => anyhow::bail!(
            "unsupported governed channel {channel}; expected stable, current, or shared-server"
        ),
    }
}

fn normalized_channel_marker(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn read_channel_marker(channel: &str) -> Result<Option<String>> {
    let path = channel_marker_path(channel)?;
    if !path.exists() {
        return Ok(None);
    }
    let metadata = std::fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "refusing to read symlinked governed channel marker {}",
            path.display()
        );
    }
    let value = std::fs::read_to_string(&path)?;
    Ok(normalized_channel_marker(Some(&value)).map(str::to_string))
}

struct ChannelCasLock {
    path: PathBuf,
    file: Option<std::fs::File>,
}

impl Drop for ChannelCasLock {
    fn drop(&mut self) {
        let _ = self.file.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

fn acquire_channel_cas_lock(channel: &str) -> Result<ChannelCasLock> {
    let lock_path = builds_dir()?.join(format!(".channel-{channel}.lock"));
    for _ in 0..200 {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(file) => {
                return Ok(ChannelCasLock {
                    path: lock_path,
                    file: Some(file),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("create governed channel lock {}", lock_path.display())
                });
            }
        }
    }
    anyhow::bail!(
        "timed out waiting for governed channel lock {}",
        lock_path.display()
    )
}

fn write_channel_marker_atomically(path: &Path, version: &str) -> Result<()> {
    if path.exists() {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            anyhow::bail!(
                "refusing to replace symlinked governed channel marker {}",
                path.display()
            );
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("governed channel marker has no parent"))?;
    storage::ensure_dir(parent)?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temp = parent.join(format!(
        ".{}.tmp-{}-{nonce}",
        binary_stem(),
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .with_context(|| format!("create temporary channel marker {}", temp.display()))?;
    use std::io::Write as _;
    file.write_all(version.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);

    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    if let Err(error) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error).with_context(|| {
            format!(
                "atomically install governed channel marker {}",
                path.display()
            )
        });
    }
    Ok(())
}

fn rollback_channel_symlink(channel: &str, previous: Option<&str>) -> Result<()> {
    let link = builds_dir()?.join(channel).join(binary_name());
    match normalized_channel_marker(previous) {
        Some(previous) => {
            update_channel_symlink(channel, previous)?;
        }
        None => {
            if link.exists() {
                std::fs::remove_file(link)?;
            }
        }
    }
    Ok(())
}

/// Promote an admitted immutable version using a governed compare-and-swap.
///
/// `expected` is the caller's observed marker. The marker is re-read while a
/// per-channel lock is held, so a stale updater fails closed instead of
/// overwriting a newer upstream automerge or operator promotion. The binary
/// symlink and marker are staged together with rollback on marker failure.
pub fn promote_channel_cas(
    channel: &str,
    version: &str,
    expected: Option<&str>,
) -> Result<Option<String>> {
    channel_marker_path(channel)?;
    require_admitted_fork_build(version)?;
    let _lock = acquire_channel_cas_lock(channel)?;
    let previous = read_channel_marker(channel)?;
    if normalized_channel_marker(expected) != normalized_channel_marker(previous.as_deref()) {
        anyhow::bail!(
            "stale governed channel state for {channel}: expected {:?}, observed {:?}",
            normalized_channel_marker(expected),
            normalized_channel_marker(previous.as_deref())
        );
    }

    update_channel_symlink(channel, version)?;
    if let Err(error) = write_channel_marker_atomically(&channel_marker_path(channel)?, version) {
        let rollback = rollback_channel_symlink(channel, previous.as_deref());
        return match rollback {
            Ok(()) => {
                Err(error).context("governed channel marker update failed; symlink rolled back")
            }
            Err(rollback_error) => Err(error).context(format!(
                "governed channel marker update failed and rollback failed: {rollback_error:#}"
            )),
        };
    }
    Ok(previous)
}

fn update_admitted_channel(channel: &str, version: &str) -> Result<PathBuf> {
    let expected = read_channel_marker(channel)?;
    promote_channel_cas(channel, version, expected.as_deref())?;
    Ok(builds_dir()?.join(channel).join(binary_name()))
}

/// Update stable symlink to point to a version and publish stable-version marker.
pub fn update_stable_symlink(version: &str) -> Result<PathBuf> {
    update_admitted_channel("stable", version)
}

pub fn update_stable_to_admitted_fork_build(version: &str) -> Result<PathBuf> {
    update_stable_symlink(version)
}

/// Update current symlink to point to a version and publish current-version marker.
pub fn update_current_symlink(version: &str) -> Result<PathBuf> {
    update_admitted_channel("current", version)
}

pub fn update_current_to_admitted_fork_build(version: &str) -> Result<PathBuf> {
    update_current_symlink(version)
}

/// Update the shared server symlink to point to a version and publish the
/// shared-server-version marker.
pub fn update_shared_server_symlink(version: &str) -> Result<PathBuf> {
    update_admitted_channel("shared-server", version)
}

pub fn update_shared_server_to_admitted_fork_build(version: &str) -> Result<PathBuf> {
    update_shared_server_symlink(version)
}

pub fn publish_local_current_build_for_source(
    repo_dir: &Path,
    source: &SourceState,
) -> Result<PublishedBuild> {
    let binary = find_dev_binary(repo_dir)
        .ok_or_else(|| anyhow::anyhow!("Binary not found in target/selfdev or target/release"))?;
    if !binary.exists() {
        anyhow::bail!("Binary not found at {:?}", binary);
    }

    validate_dev_binary_matches_source(repo_dir, &binary, source)?;
    let previous_current_version = read_current_version()?;
    let versioned_path = install_binary_at_version(&binary, &source.version_label)?;
    let installed_report = read_binary_version_report(&versioned_path)?;
    if installed_report
        .version
        .as_deref()
        .unwrap_or_default()
        .is_empty()
    {
        anyhow::bail!(
            "Binary smoke test for {} returned JSON without a version field",
            versioned_path.display()
        );
    }
    validate_binary_version_matches_source_report(&installed_report, &versioned_path, source)?;
    write_immutable_source_metadata(&versioned_path, source)?;
    let bootstrapped = bootstrap_legacy_fork_build(repo_dir)?;
    let predecessor = match bootstrapped {
        Some(admission) => Some(admission.version),
        None if any_fork_admission_receipt_exists()? => admitted_release_line_head()?,
        None => None,
    };
    if predecessor.is_none() && !any_fork_admission_receipt_exists()? {
        admit_initial_local_fork_build(repo_dir, source)?;
    } else {
        let predecessor = predecessor.ok_or_else(|| {
            anyhow::anyhow!("admitted fork receipts exist without a configured release-line head")
        })?;
        admit_installed_fork_build_with_root(
            &source.version_label,
            &format!(
                "local-fork:{CANONICAL_FORK_REPOSITORY}:{}",
                source.repo_scope
            ),
            Some(&predecessor),
            repo_dir,
        )?;
    }
    let current_link = update_current_to_admitted_fork_build(&source.version_label)?;
    let launcher_link = update_launcher_symlink_to_current()?;

    Ok(PublishedBuild {
        version: source.version_label.clone(),
        source_fingerprint: source.fingerprint.clone(),
        versioned_path,
        current_link,
        launcher_link,
        previous_current_version,
    })
}

/// Install the local release binary into immutable versions and make it the active `current`
/// build + launcher, while keeping `stable` untouched.
pub fn publish_local_current_build(repo_dir: &std::path::Path) -> Result<PathBuf> {
    let source = current_source_state(repo_dir)?;
    Ok(publish_local_current_build_for_source(repo_dir, &source)?.versioned_path)
}

/// Promote an already installed immutable version onto the shared server channel.
pub fn promote_version_to_shared_server(version: &str) -> Result<Option<String>> {
    let previous = read_shared_server_version()?;
    promote_channel_cas("shared-server", version, previous.as_deref())
}

/// Returns true when the `shared-server` channel is merely tracking the
/// `stable` channel rather than pinned to a deliberately-promoted build (e.g. a
/// local self-dev binary).
///
/// Updates only advance `current`/`stable`, so the long-lived daemon's reload
/// target (`shared-server`) can drift behind an update. When the channel was
/// just following stable we want updates to carry it forward automatically;
/// when it was explicitly promoted to a self-dev build we must leave it alone
/// so an update never silently wipes that build out from under a force reload.
///
/// A never-promoted (missing/empty) shared-server marker counts as "tracking":
/// there is no deliberate build to protect, so it is safe for updates to begin
/// populating the channel.
pub fn shared_server_tracks_stable() -> Result<bool> {
    let shared = read_shared_server_version()?;
    let shared = shared.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let Some(shared) = shared else {
        return Ok(true);
    };
    let stable = read_stable_version()?;
    let stable = stable.as_deref().map(str::trim).filter(|s| !s.is_empty());
    Ok(stable == Some(shared))
}

/// Returns true when the `current` channel is merely tracking the `stable`
/// channel rather than pinned to a deliberately-published local build.
///
/// This is the launcher-facing counterpart to [`shared_server_tracks_stable`].
/// `publish_local_current_build_for_source` points `current` (and the user's
/// launcher symlink) at a self-dev version label such as
/// `<hash>-dirty-<digest>`; a stable-channel auto-update that then
/// unconditionally rewrites `current` silently deletes that build out from
/// under the user, taking every not-yet-upstreamed tool with it. When
/// `current` matches `stable` it is just following updates and should keep
/// doing so; when it differs it was deliberately pinned and must be left
/// alone.
///
/// A missing/empty `current` marker counts as "tracking": there is no
/// deliberate build to protect.
pub fn current_tracks_stable() -> Result<bool> {
    let current = read_current_version()?;
    let current = current.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let Some(current) = current else {
        return Ok(true);
    };
    let stable = read_stable_version()?;
    let stable = stable.as_deref().map(str::trim).filter(|s| !s.is_empty());
    Ok(stable == Some(current))
}

/// Advance the `current` channel (and launcher symlink) to `version`, but only
/// when it is currently tracking `stable` (see [`current_tracks_stable`]).
/// Returns `Ok(true)` when the channel was advanced.
///
/// Callers in the update path MUST invoke this *before* moving the `stable`
/// marker, otherwise the pre-update comparison would always disagree.
pub fn advance_current_if_tracking_stable(version: &str) -> Result<bool> {
    if current_tracks_stable()? {
        validate_existing_channel_marker(read_current_version()?, "current")?;
        update_current_to_admitted_fork_build(version)?;
        update_launcher_symlink_to_current()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Advance the `shared-server` channel to `version`, but only when it is
/// currently tracking `stable` (see [`shared_server_tracks_stable`]). Returns
/// `Ok(true)` when the channel was advanced.
///
/// Callers in the update path MUST invoke this *before* moving the `stable`
/// marker, otherwise the pre-update comparison would always disagree.
pub fn advance_shared_server_if_tracking_stable(version: &str) -> Result<bool> {
    if shared_server_tracks_stable()? {
        validate_existing_channel_marker(read_shared_server_version()?, "shared-server")?;
        update_shared_server_to_admitted_fork_build(version)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn validate_existing_channel_marker(marker: Option<String>, channel: &str) -> Result<()> {
    let Some(version) = marker
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    require_admitted_fork_build(version).map_err(|error| {
        anyhow::anyhow!(
            "refusing to advance {channel} channel from inadmissible build {version}: {error}"
        )
    })?;
    Ok(())
}

/// Outcome of [`repair_stale_shared_server_channel`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SharedServerRepair {
    /// The `shared-server` channel was repointed at the installed `stable`
    /// release because stable was strictly newer on disk.
    Repaired {
        previous: Option<String>,
        repaired_to: String,
    },
    /// Nothing to do: shared-server is already at/newer than stable, or there is
    /// no usable stable target.
    AlreadyCurrent,
}

/// Drag a *stale* `shared-server` channel forward to the installed `stable`
/// release so a long-lived daemon can actually reload into a newer binary.
///
/// This is the client-side counterpart to [`advance_shared_server_if_tracking_stable`].
/// Updates advance `stable` but only advance `shared-server` *during the install
/// path*; a client that is already on the newest release (so `/update` is a
/// no-op) never re-runs that install path, leaving a long-lived older daemon
/// pinned to its old `shared-server` binary forever. A newer client that detects
/// an older server calls this to repoint `shared-server` -> `stable` before
/// asking the server to reload, so the forced reload has a strictly-newer target
/// to exec into instead of re-execing the same old binary (the "current client,
/// stale server" report).
///
/// Safety: we only repair when the `stable` binary is *strictly newer by mtime*
/// than the current `shared-server` binary. That preserves a deliberately-pinned
/// self-dev `shared-server` build whenever it is at least as fresh as stable (the
/// case the pin exists to protect), and never downgrades the channel.
pub fn repair_stale_shared_server_channel() -> Result<SharedServerRepair> {
    let stable_version = read_stable_version()?;
    let Some(stable_version) = stable_version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        return Ok(SharedServerRepair::AlreadyCurrent);
    };

    let stable_binary = stable_binary_path()?;
    if !stable_binary.exists() {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }
    if require_admitted_fork_build(stable_version).is_err() {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }

    // If shared-server already resolves to the same version marker, there is
    // nothing to repair.
    let previous = read_shared_server_version()?;
    if previous.as_deref().map(str::trim).filter(|s| !s.is_empty()) == Some(stable_version) {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }
    if previous
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some_and(|previous| !is_release_channel_marker(previous))
    {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }
    if previous
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some_and(|previous| require_admitted_fork_build(previous).is_err())
    {
        // An unverifiable pin is never a repair target. Replacing it with
        // stable would turn repair into an admission bypass and could silently
        // discard an operator's emergency recovery build.
        return Ok(SharedServerRepair::AlreadyCurrent);
    }

    // Only repair when stable is strictly newer than the current shared-server
    // binary on disk. This never downgrades, and it preserves a self-dev pin
    // that is fresher than stable.
    let shared_binary = shared_server_binary_path()?;
    if !shared_server_binary_is_strictly_older_than(&shared_binary, &stable_binary) {
        return Ok(SharedServerRepair::AlreadyCurrent);
    }

    update_shared_server_to_admitted_fork_build(stable_version)?;
    Ok(SharedServerRepair::Repaired {
        previous: previous
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        repaired_to: stable_version.to_string(),
    })
}

fn is_release_channel_marker(marker: &str) -> bool {
    let marker = marker.trim();
    let marker = marker.strip_prefix('v').unwrap_or(marker);
    marker.starts_with("main-")
        || marker
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

/// True when `shared` exists and is strictly older (by mtime) than `stable`, or
/// when `shared` is missing entirely (nothing to protect). Any mtime
/// uncertainty on an existing shared binary is treated as "not older" so we
/// never repair away an unverifiable (possibly newer) pinned build.
///
/// Both paths are resolved through [`resolve_binary_payload`] so release
/// installs (wrapper script + `.bin` payload) compare the payloads that
/// actually run instead of the tiny wrapper scripts, whose mtimes carry no
/// version information.
fn shared_server_binary_is_strictly_older_than(
    shared: &std::path::Path,
    stable: &std::path::Path,
) -> bool {
    let mtime = |p: &std::path::Path| {
        std::fs::metadata(resolve_binary_payload(p))
            .ok()
            .and_then(|m| m.modified().ok())
    };
    let stable_mtime = match mtime(stable) {
        Some(m) => m,
        None => return false,
    };
    if !shared.exists() {
        // No deliberate pin on disk; safe to point the channel at stable.
        return true;
    }
    match mtime(shared) {
        Some(shared_mtime) => shared_mtime < stable_mtime,
        None => false,
    }
}

/// Install release binary into immutable versions, promote it to stable, and also make it the
/// active current/launcher build.
pub fn install_local_release(repo_dir: &std::path::Path) -> Result<PathBuf> {
    let source = release_binary_path(repo_dir);
    if !source.exists() {
        anyhow::bail!("Binary not found at {:?}", source);
    }

    let version = repo_build_version(repo_dir)?;

    let versioned = install_binary_at_version(&source, &version)?;
    let predecessor = admitted_release_line_head()?;
    admit_installed_fork_build_with_root(
        &version,
        &format!("local-fork:{CANONICAL_FORK_REPOSITORY}:release"),
        predecessor.as_deref(),
        repo_dir,
    )?;
    update_stable_to_admitted_fork_build(&version)?;
    update_current_to_admitted_fork_build(&version)?;
    update_shared_server_to_admitted_fork_build(&version)?;
    update_launcher_symlink_to_current()?;

    Ok(versioned)
}

/// Copy binary to versioned location
pub fn install_version(repo_dir: &std::path::Path, hash: &str) -> Result<PathBuf> {
    let source = release_binary_path(repo_dir);
    install_binary_at_version(&source, hash)
}

/// Update canary symlink to point to a version
pub fn update_canary_symlink(hash: &str) -> Result<()> {
    require_admitted_fork_build(hash)?;
    let _ = update_channel_symlink("canary", hash)?;
    Ok(())
}

#[cfg(test)]
mod tests;
