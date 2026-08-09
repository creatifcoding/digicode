//! Durable governance for capabilities authored in the maintained fork.
//!
//! The fork used to advertise only two names (`mt` and `tasker`).  Names are
//! not a preservation contract: an upstream automerge can keep a name while
//! deleting an API, changing behaviour, or dropping a runtime asset.  This
//! module makes the generated manifest the source of truth for admission.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const MANIFEST_SCHEMA_VERSION: u32 = 1;
pub const RECOVERY_BASELINE_SCHEMA_VERSION: u32 = 1;
pub const MANIFEST_FILE_NAME: &str = "capability-manifest.json";
pub const MANIFEST_SOURCE: &str = include_str!("../../../docs/FORK_CAPABILITY_MANIFEST.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractSpec {
    pub summary: String,
    #[serde(default)]
    pub references: Vec<String>,
}

impl ContractSpec {
    pub fn named(name: &str, summary: &str) -> Self {
        Self {
            summary: format!("{name}: {summary}"),
            references: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityManifestEntry {
    pub id: String,
    pub display_name: String,
    pub owner: String,
    pub introduced_commit: String,
    pub lifecycle: String,
    /// Product/readiness state recovered from the maintained fork inventory.
    /// This is intentionally separate from `lifecycle`: an active manifest
    /// entry can still be source-only, standalone, or gated.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recovery_state: String,
    #[serde(default)]
    pub source_paths: Vec<String>,
    #[serde(default)]
    pub asset_paths: Vec<String>,
    pub schema_contract: ContractSpec,
    pub api_contract: ContractSpec,
    pub behavior_contract: ContractSpec,
    pub migration_contract: ContractSpec,
    pub asset_contract: ContractSpec,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecoveryBaseline {
    pub schema_version: u32,
    pub baseline_id: String,
    pub scope: String,
    pub indexed_documents: Vec<String>,
    pub capability_ids: Vec<String>,
}

impl RecoveryBaseline {
    fn validate(&self) -> Result<()> {
        if self.schema_version != RECOVERY_BASELINE_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported recovery baseline schema {} (expected {})",
                self.schema_version,
                RECOVERY_BASELINE_SCHEMA_VERSION
            );
        }
        validate_id(&self.baseline_id, "recovery baseline")?;
        if self.scope != "source-presence-and-readiness" {
            anyhow::bail!("unsupported recovery baseline scope {:?}", self.scope);
        }
        if self.indexed_documents.is_empty()
            || self
                .indexed_documents
                .iter()
                .any(|document| document.trim().is_empty() || !document.starts_with("recovery:"))
        {
            anyhow::bail!(
                "recovery baseline must contain non-empty indexed recovery document references"
            );
        }
        let mut documents = HashSet::new();
        if self
            .indexed_documents
            .iter()
            .any(|document| !documents.insert(document.as_str()))
        {
            anyhow::bail!("recovery baseline contains duplicate indexed documents");
        }

        if self.capability_ids.is_empty() {
            anyhow::bail!("recovery baseline omitted capability IDs");
        }
        let mut capabilities = HashSet::new();
        for capability_id in &self.capability_ids {
            validate_id(capability_id, "recovery baseline capability")?;
            if !capabilities.insert(capability_id.as_str()) {
                anyhow::bail!(
                    "recovery baseline contains duplicate capability {}",
                    capability_id
                );
            }
        }
        Ok(())
    }
}

impl CapabilityManifestEntry {
    pub fn legacy(id: &str) -> Self {
        let summary =
            format!("legacy capability {id}; preserved as an explicit migration contract");
        let contract = ContractSpec {
            summary: summary.clone(),
            references: vec!["legacy:flat-capability-report".to_string()],
        };
        Self {
            id: id.to_string(),
            display_name: id.to_string(),
            owner: "maintained-fork".to_string(),
            introduced_commit: "legacy".to_string(),
            lifecycle: "legacy-migrated".to_string(),
            recovery_state: String::new(),
            source_paths: Vec::new(),
            asset_paths: Vec::new(),
            schema_contract: contract.clone(),
            api_contract: contract.clone(),
            behavior_contract: contract.clone(),
            migration_contract: contract.clone(),
            asset_contract: contract,
            evidence: vec!["legacy:fork-admission.json".to_string()],
        }
    }

    fn contract_fingerprint(&self) -> Result<String> {
        digest_json(&(
            &self.id,
            &self.schema_contract,
            &self.api_contract,
            &self.behavior_contract,
            &self.migration_contract,
            &self.asset_contract,
            &self.recovery_state,
            &self.source_paths,
            &self.asset_paths,
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetirementRecord {
    pub capability_id: String,
    pub retirement_id: String,
    pub recorded_commit: String,
    pub effective_version: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub successor_id: Option<String>,
    pub contract_digest: String,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_retirement_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForkCapabilityManifest {
    pub schema_version: u32,
    pub manifest_version: String,
    pub generated_from_commit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_baseline: Option<RecoveryBaseline>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_manifest_sha256: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<CapabilityManifestEntry>,
    #[serde(default)]
    pub retirements: Vec<RetirementRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl ForkCapabilityManifest {
    pub fn without_digest(&self) -> Self {
        let mut copy = self.clone();
        copy.sha256 = None;
        copy
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(&self.without_digest())
            .context("serialize canonical capability manifest")
    }

    pub fn digest(&self) -> Result<String> {
        digest_bytes(&self.canonical_bytes()?)
    }

    /// Validate that an installed manifest is fresh with respect to the
    /// manifest digest reported by its binary at runtime.
    ///
    /// Admission adds `predecessor_manifest_sha256` when it links a new
    /// release to the existing release line. That linkage is intentionally
    /// excluded from the freshness comparison. Every other manifest field is
    /// part of the runtime identity and must match exactly.
    pub fn validate_freshness(&self, runtime_digest: &str) -> Result<()> {
        self.validate()?;
        let runtime_digest = runtime_digest.trim();
        if runtime_digest.is_empty() {
            anyhow::bail!("runtime report omitted the capability manifest digest");
        }

        let actual = self.digest()?;
        if actual == runtime_digest {
            return Ok(());
        }

        let mut admission_linked = self.clone();
        admission_linked.predecessor_manifest_sha256 = None;
        if admission_linked.digest()? == runtime_digest {
            return Ok(());
        }

        anyhow::bail!(
            "fork capability manifest is stale: runtime reported digest {}, installed manifest digest {}",
            runtime_digest,
            actual
        )
    }

    pub fn with_digest(mut self) -> Result<Self> {
        self.sha256 = Some(self.digest()?);
        Ok(self)
    }

    /// Pre-baseline manifests were written before recovery readiness became a
    /// frozen contract. Treat those manifests as legacy during migration.
    pub fn is_legacy(&self) -> bool {
        self.manifest_version.starts_with("legacy-")
            || (self.recovery_baseline.is_none()
                && self
                    .capabilities
                    .iter()
                    .all(|capability| capability.recovery_state.is_empty()))
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            anyhow::bail!(
                "unsupported fork capability manifest schema {} (expected {})",
                self.schema_version,
                MANIFEST_SCHEMA_VERSION
            );
        }
        if self.manifest_version.trim().is_empty() {
            anyhow::bail!("fork capability manifest omitted manifest_version");
        }
        if self.generated_from_commit.trim().is_empty() {
            anyhow::bail!("fork capability manifest omitted generated_from_commit");
        }
        if let Some(baseline) = &self.recovery_baseline {
            baseline.validate()?;
        }

        let has_recovery_baseline = self.recovery_baseline.is_some();
        let mut ids = HashSet::new();
        for capability in &self.capabilities {
            validate_id(&capability.id, "capability")?;
            if !ids.insert(capability.id.clone()) {
                anyhow::bail!("duplicate fork capability id {}", capability.id);
            }
            if capability.display_name.trim().is_empty()
                || capability.owner.trim().is_empty()
                || capability.lifecycle.trim().is_empty()
            {
                anyhow::bail!(
                    "capability {} has incomplete identity metadata",
                    capability.id
                );
            }
            if !capability.recovery_state.is_empty()
                && !RECOVERY_STATES.contains(&capability.recovery_state.as_str())
            {
                anyhow::bail!(
                    "capability {} has unsupported recovery state {:?}",
                    capability.id,
                    capability.recovery_state
                );
            }
            if has_recovery_baseline && capability.recovery_state.is_empty() {
                anyhow::bail!(
                    "capability {} omitted its frozen recovery state",
                    capability.id
                );
            }
            if has_recovery_baseline && capability.evidence.is_empty() {
                anyhow::bail!(
                    "capability {} omitted evidence for the frozen recovery baseline",
                    capability.id
                );
            }
            validate_paths(&capability.source_paths, "source", &capability.id)?;
            validate_paths(&capability.asset_paths, "asset", &capability.id)?;
            for (name, contract) in [
                ("schema", &capability.schema_contract),
                ("api", &capability.api_contract),
                ("behavior", &capability.behavior_contract),
                ("migration", &capability.migration_contract),
                ("asset", &capability.asset_contract),
            ] {
                if contract.summary.trim().is_empty() {
                    anyhow::bail!("capability {} has an empty {name} contract", capability.id);
                }
            }
        }

        let mut retirement_ids = HashSet::new();
        let mut retired_capabilities = HashSet::new();
        for retirement in &self.retirements {
            validate_id(&retirement.capability_id, "retired capability")?;
            validate_id(&retirement.retirement_id, "retirement")?;
            if !retirement_ids.insert(retirement.retirement_id.clone()) {
                anyhow::bail!("duplicate retirement id {}", retirement.retirement_id);
            }
            if !retired_capabilities.insert(retirement.capability_id.clone()) {
                anyhow::bail!(
                    "capability {} has multiple retirement records",
                    retirement.capability_id
                );
            }
            if ids.contains(&retirement.capability_id) {
                anyhow::bail!(
                    "retired capability {} is reactivated in the same manifest",
                    retirement.capability_id
                );
            }
            if retirement.recorded_commit.trim().is_empty()
                || retirement.effective_version.trim().is_empty()
                || retirement.reason.trim().is_empty()
                || retirement.contract_digest.trim().is_empty()
                || retirement.evidence.is_empty()
            {
                anyhow::bail!("retirement {} is incomplete", retirement.retirement_id);
            }
            if retirement.successor_id.as_deref() == Some(retirement.capability_id.as_str()) {
                anyhow::bail!("retirement {} points to itself", retirement.retirement_id);
            }
        }

        if let Some(baseline) = &self.recovery_baseline {
            let known_ids = ids
                .iter()
                .map(String::as_str)
                .chain(retired_capabilities.iter().map(String::as_str))
                .collect::<HashSet<_>>();
            if let Some(unknown) = baseline
                .capability_ids
                .iter()
                .find(|capability_id| !known_ids.contains(capability_id.as_str()))
            {
                anyhow::bail!(
                    "recovery baseline references unknown capability {}",
                    unknown
                );
            }
        }

        if let Some(expected) = self.sha256.as_deref() {
            let actual = self.digest()?;
            if expected != actual {
                anyhow::bail!(
                    "fork capability manifest digest mismatch: declared {}, actual {}",
                    expected,
                    actual
                );
            }
        }
        Ok(())
    }

    pub fn active_by_id(&self) -> HashMap<&str, &CapabilityManifestEntry> {
        self.capabilities
            .iter()
            .map(|capability| (capability.id.as_str(), capability))
            .collect()
    }

    /// The checked-in generated inventory is the protected baseline for a new
    /// release line. A manifest may remove one of these entries only when its
    /// own immutable retirement ledger contains the exact capability ID.
    pub fn validate_baseline(&self, baseline: &Self) -> Result<()> {
        let candidate_is_legacy = self.is_legacy();
        if !candidate_is_legacy && let Some(expected) = &baseline.recovery_baseline {
            match &self.recovery_baseline {
                Some(actual) if actual == expected => {}
                Some(_) => {
                    anyhow::bail!("candidate changed the frozen recovery capability baseline")
                }
                None => anyhow::bail!("candidate omitted the frozen recovery capability baseline"),
            }
        }
        let active = self.active_by_id();
        let retired = self
            .retirements
            .iter()
            .map(|record| record.capability_id.as_str())
            .collect::<HashSet<_>>();
        for capability in &baseline.capabilities {
            if let Some(candidate) = active.get(capability.id.as_str())
                && !candidate_is_legacy
                && baseline.recovery_baseline.is_some()
                && candidate.recovery_state != capability.recovery_state
            {
                anyhow::bail!(
                    "candidate changed the frozen recovery state for capability {}",
                    capability.id
                );
            }
            if !active.contains_key(capability.id.as_str())
                && !retired.contains(capability.id.as_str())
            {
                anyhow::bail!(
                    "manifest dropped baseline fork capability {} without retirement",
                    capability.id
                );
            }
        }
        Ok(())
    }

    /// The checked-in manifest is the authoritative recovery baseline. Keep
    /// its inventory and readiness states explicit so `lifecycle: active`
    /// cannot be mistaken for runtime activation or complete product wiring.
    pub fn validate_frozen_baseline(&self) -> Result<()> {
        self.validate()?;
        let baseline = self
            .recovery_baseline
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("manifest omitted its frozen recovery baseline"))?;
        let capability_ids = self
            .capabilities
            .iter()
            .map(|capability| capability.id.clone())
            .collect::<Vec<_>>();
        if baseline.capability_ids != capability_ids {
            anyhow::bail!("recovery baseline capability IDs do not match the manifest inventory");
        }
        for capability in &self.capabilities {
            if capability.recovery_state.is_empty() {
                anyhow::bail!(
                    "capability {} omitted its frozen recovery state",
                    capability.id
                );
            }
            if capability.evidence.is_empty() {
                anyhow::bail!(
                    "capability {} omitted evidence for the frozen recovery baseline",
                    capability.id
                );
            }
        }
        Ok(())
    }

    /// Validate that every checked-in source and asset anchor exists in the
    /// canonical checkout used to produce an admitted build. The manifest is
    /// intentionally path-based so an upstream automerge that drops a file
    /// cannot pass admission merely by retaining a capability name.
    pub fn validate_source_paths(&self, root: &Path) -> Result<()> {
        self.validate()?;
        for capability in &self.capabilities {
            for path in capability
                .source_paths
                .iter()
                .chain(capability.asset_paths.iter())
            {
                let candidate = root.join(path);
                if !candidate.is_file() {
                    anyhow::bail!(
                        "capability {} references missing manifest path {}",
                        capability.id,
                        candidate.display()
                    );
                }
            }
        }
        Ok(())
    }

    /// Validate that a candidate preserves every predecessor capability and
    /// retirement record, except for capabilities explicitly retired in the
    /// candidate manifest.
    pub fn validate_transition(
        predecessor: &Self,
        candidate: &Self,
        candidate_version: &str,
    ) -> Result<()> {
        predecessor.validate()?;
        candidate.validate()?;
        let predecessor_digest = predecessor.digest()?;
        if candidate.predecessor_manifest_sha256.as_deref() != Some(predecessor_digest.as_str()) {
            anyhow::bail!(
                "candidate manifest does not name the exact predecessor manifest digest {}",
                predecessor_digest
            );
        }

        let candidate_by_id = candidate.active_by_id();
        let retirements = candidate
            .retirements
            .iter()
            .map(|record| (record.capability_id.as_str(), record))
            .collect::<HashMap<_, _>>();
        for previous in &predecessor.capabilities {
            match candidate_by_id.get(previous.id.as_str()) {
                Some(current) => {
                    let previous_fingerprint = previous.contract_fingerprint()?;
                    let current_fingerprint = current.contract_fingerprint()?;
                    if previous_fingerprint != current_fingerprint {
                        anyhow::bail!(
                            "capability {} contract changed without immutable retirement",
                            previous.id
                        );
                    }
                }
                None => match retirements.get(previous.id.as_str()) {
                    Some(retirement) => {
                        let expected_digest = previous.contract_fingerprint()?;
                        if retirement.contract_digest != expected_digest {
                            anyhow::bail!(
                                "retirement {} does not preserve capability {} contract digest",
                                retirement.retirement_id,
                                previous.id
                            );
                        }
                        if retirement.effective_version != candidate_version {
                            anyhow::bail!(
                                "retirement {} is effective at {}, not candidate {}",
                                retirement.retirement_id,
                                retirement.effective_version,
                                candidate_version
                            );
                        }
                    }
                    None => anyhow::bail!(
                        "candidate {} dropped admitted capability {} without retirement",
                        candidate_version,
                        previous.id
                    ),
                },
            }
        }

        let previous_retirements = predecessor
            .retirements
            .iter()
            .map(|record| (record.retirement_id.as_str(), record))
            .collect::<HashMap<_, _>>();
        let current_retirements = candidate
            .retirements
            .iter()
            .map(|record| (record.retirement_id.as_str(), record))
            .collect::<HashMap<_, _>>();
        for (id, previous) in previous_retirements {
            if current_retirements.get(id).copied() != Some(previous) {
                anyhow::bail!("retirement record {id} was deleted or mutated");
            }
        }
        Ok(())
    }
}

pub fn digest_bytes(bytes: &[u8]) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn digest_json<T: Serialize>(value: &T) -> Result<String> {
    digest_bytes(&serde_json::to_vec(value).context("serialize governance digest input")?)
}

pub fn builtin_manifest() -> Result<ForkCapabilityManifest> {
    let manifest: ForkCapabilityManifest = serde_json::from_str(MANIFEST_SOURCE)
        .context("parse generated fork capability manifest")?;
    manifest.validate_frozen_baseline()?;
    Ok(manifest)
}

pub fn builtin_capability_ids() -> Result<Vec<String>> {
    let manifest = builtin_manifest()?;
    Ok(manifest
        .capabilities
        .into_iter()
        .map(|capability| capability.id)
        .collect())
}

pub fn legacy_manifest(capabilities: &[String], commit: &str) -> Result<ForkCapabilityManifest> {
    let mut ids = capabilities.to_vec();
    ids.sort();
    ids.dedup();
    ForkCapabilityManifest {
        schema_version: MANIFEST_SCHEMA_VERSION,
        manifest_version: "legacy-1".to_string(),
        generated_from_commit: if commit.trim().is_empty() {
            "legacy".to_string()
        } else {
            commit.to_string()
        },
        recovery_baseline: None,
        predecessor_manifest_sha256: None,
        capabilities: ids
            .iter()
            .map(|id| CapabilityManifestEntry::legacy(id))
            .collect(),
        retirements: Vec::new(),
        sha256: None,
    }
    .with_digest()
}

pub fn manifest_path_for_binary(binary: &Path) -> PathBuf {
    binary
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(MANIFEST_FILE_NAME)
}

pub fn read_manifest_for_binary(binary: &Path) -> Result<Option<ForkCapabilityManifest>> {
    let path = manifest_path_for_binary(binary);
    if !path.exists() {
        return Ok(None);
    }
    let manifest: ForkCapabilityManifest = serde_json::from_slice(
        &std::fs::read(&path)
            .with_context(|| format!("read capability manifest {}", path.display()))?,
    )
    .with_context(|| format!("parse capability manifest {}", path.display()))?;
    manifest.validate()?;
    Ok(Some(manifest))
}

pub fn write_immutable_manifest(binary: &Path, manifest: &ForkCapabilityManifest) -> Result<()> {
    manifest.validate()?;
    let path = manifest_path_for_binary(binary);
    let bytes = serde_json::to_vec_pretty(&manifest.clone().with_digest()?)?;
    if path.exists() {
        let existing = std::fs::read(&path)?;
        if existing != bytes {
            anyhow::bail!(
                "refusing to replace immutable capability manifest {}",
                path.display()
            );
        }
        return Ok(());
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

pub fn load_manifest_or_legacy(
    binary: &Path,
    capabilities: &[String],
    git_hash: &str,
) -> Result<(ForkCapabilityManifest, bool)> {
    if let Some(manifest) = read_manifest_for_binary(binary)? {
        return Ok((manifest, false));
    }
    Ok((legacy_manifest(capabilities, git_hash)?, true))
}

fn validate_id(value: &str, kind: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        anyhow::bail!("invalid {kind} id {value:?}");
    }
    Ok(())
}

const RECOVERY_STATES: &[&str] = &[
    "live",
    "source-live",
    "partial-gated",
    "standalone",
    "source-only",
    "proposed",
    "superseded",
];

fn validate_paths(paths: &[String], kind: &str, capability: &str) -> Result<()> {
    for path in paths {
        let path = Path::new(path);
        if path.is_absolute()
            || path
                .components()
                .any(|component| component == std::path::Component::ParentDir)
        {
            anyhow::bail!(
                "capability {capability} has unsafe {kind} path {}",
                path.display()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: &str) -> CapabilityManifestEntry {
        CapabilityManifestEntry {
            id: id.to_string(),
            display_name: id.to_string(),
            owner: "fork".to_string(),
            introduced_commit: "abc1234".to_string(),
            lifecycle: "active".to_string(),
            recovery_state: String::new(),
            source_paths: vec![format!("src/{id}.rs")],
            asset_paths: vec![format!("assets/{id}.json")],
            schema_contract: ContractSpec::named("schema", "stable schema"),
            api_contract: ContractSpec::named("api", "stable api"),
            behavior_contract: ContractSpec::named("behavior", "stable behavior"),
            migration_contract: ContractSpec::named("migration", "stable migration"),
            asset_contract: ContractSpec::named("asset", "stable asset"),
            evidence: vec![format!("test:{id}")],
        }
    }

    fn manifest(ids: &[&str]) -> ForkCapabilityManifest {
        ForkCapabilityManifest {
            schema_version: MANIFEST_SCHEMA_VERSION,
            manifest_version: "1".to_string(),
            generated_from_commit: "abc1234".to_string(),
            recovery_baseline: None,
            predecessor_manifest_sha256: None,
            capabilities: ids.iter().map(|id| sample(id)).collect(),
            retirements: Vec::new(),
            sha256: None,
        }
    }

    #[test]
    fn manifest_digest_is_deterministic_and_self_validating() {
        let manifest = manifest(&["alpha"]).with_digest().unwrap();
        let digest = manifest.digest().unwrap();
        assert_eq!(manifest.sha256.as_deref(), Some(digest.as_str()));
        manifest.validate().unwrap();
    }

    #[test]
    fn manifest_freshness_allows_admission_linkage_but_rejects_contract_drift() {
        let baseline = manifest(&["alpha"]).with_digest().unwrap();
        let runtime_digest = baseline.digest().unwrap();

        let mut linked = baseline.clone();
        linked.predecessor_manifest_sha256 = Some("predecessor-digest".to_string());
        linked = linked.with_digest().unwrap();
        linked
            .validate_freshness(&runtime_digest)
            .expect("admission-owned predecessor linkage should be allowed");

        let mut stale = linked;
        stale.capabilities[0].behavior_contract.summary = "drifted".to_string();
        stale = stale.with_digest().unwrap();
        let error = stale
            .validate_freshness(&runtime_digest)
            .expect_err("contract drift must make the sidecar stale");
        assert!(error.to_string().contains("manifest is stale"));
    }

    #[test]
    fn transition_rejects_contract_loss_even_when_id_survives() {
        let predecessor = manifest(&["alpha"]).with_digest().unwrap();
        let mut candidate = manifest(&["alpha"]);
        candidate.capabilities[0].behavior_contract.summary = "changed".to_string();
        candidate.predecessor_manifest_sha256 = Some(predecessor.digest().unwrap());
        let error = ForkCapabilityManifest::validate_transition(&predecessor, &candidate, "2")
            .expect_err("contract loss must fail");
        assert!(error.to_string().contains("contract changed"));
    }

    #[test]
    fn transition_accepts_explicit_retirement_and_rejects_reactivation() {
        let predecessor = manifest(&["alpha"]).with_digest().unwrap();
        let contract_digest = predecessor.capabilities[0].contract_fingerprint().unwrap();
        let retirement = RetirementRecord {
            capability_id: "alpha".to_string(),
            retirement_id: "retire-alpha-2".to_string(),
            recorded_commit: "def5678".to_string(),
            effective_version: "2".to_string(),
            reason: "replaced".to_string(),
            successor_id: Some("beta".to_string()),
            contract_digest,
            evidence: vec!["test:retirement".to_string()],
            predecessor_retirement_digest: None,
        };
        let mut candidate = manifest(&[]);
        candidate.predecessor_manifest_sha256 = Some(predecessor.digest().unwrap());
        candidate.retirements = vec![retirement.clone()];
        ForkCapabilityManifest::validate_transition(&predecessor, &candidate, "2").unwrap();

        let mut reactivated = candidate;
        reactivated.capabilities = vec![sample("alpha")];
        assert!(reactivated.validate().is_err());
    }

    #[test]
    fn builtin_manifest_is_complete_and_not_two_names() {
        let manifest = builtin_manifest().unwrap();
        assert!(manifest.capabilities.len() >= 3);
        assert!(
            manifest
                .capabilities
                .iter()
                .any(|capability| capability.id == "mt")
        );
        assert!(
            manifest
                .capabilities
                .iter()
                .any(|capability| capability.id == "tasker")
        );
    }

    #[test]
    fn builtin_manifest_freezes_recovery_inventory_and_readiness() {
        let manifest = builtin_manifest().unwrap();
        let baseline = manifest
            .recovery_baseline
            .as_ref()
            .expect("built-in manifest recovery baseline");

        assert_eq!(baseline.schema_version, RECOVERY_BASELINE_SCHEMA_VERSION);
        assert_eq!(baseline.scope, "source-presence-and-readiness");
        assert_eq!(
            baseline.indexed_documents,
            vec![
                "recovery:digicode-fork-capability-recovery.md",
                "recovery:jcode-tui-rendering-and-alt-m-workspace.md",
                "recovery:jcode-workstream-control-board.md",
            ]
        );
        assert!(
            manifest
                .capabilities
                .iter()
                .all(|capability| RECOVERY_STATES.contains(&capability.recovery_state.as_str()))
        );
        assert!(
            manifest
                .capabilities
                .iter()
                .all(|capability| !capability.evidence.is_empty())
        );
    }

    #[test]
    fn frozen_baseline_rejects_inventory_drift() {
        let mut manifest = builtin_manifest().unwrap();
        manifest
            .recovery_baseline
            .as_mut()
            .unwrap()
            .capability_ids
            .pop();
        manifest.sha256 = None;
        let error = manifest
            .validate_frozen_baseline()
            .expect_err("baseline inventory drift must be rejected");
        assert!(
            error
                .to_string()
                .contains("recovery baseline capability IDs do not match")
        );
    }

    #[test]
    fn candidate_cannot_change_frozen_recovery_baseline() {
        let baseline = builtin_manifest().unwrap();
        let mut candidate = baseline.clone();
        candidate
            .recovery_baseline
            .as_mut()
            .unwrap()
            .indexed_documents
            .pop();
        let error = candidate
            .validate_baseline(&baseline)
            .expect_err("candidate baseline drift must be rejected");
        assert!(
            error
                .to_string()
                .contains("changed the frozen recovery capability baseline")
        );
    }

    #[test]
    fn pre_baseline_manifest_is_accepted_during_migration() {
        let baseline = builtin_manifest().unwrap();
        let mut candidate = baseline.clone();
        candidate.recovery_baseline = None;
        for capability in &mut candidate.capabilities {
            capability.recovery_state.clear();
        }
        let candidate = candidate.with_digest().unwrap();

        assert!(candidate.is_legacy());
        candidate
            .validate_baseline(&baseline)
            .expect("pre-baseline manifests must remain admissible during migration");
    }

    #[test]
    fn candidate_cannot_change_frozen_recovery_state() {
        let baseline = builtin_manifest().unwrap();
        let mut candidate = baseline.clone();
        candidate.capabilities[0].recovery_state =
            if candidate.capabilities[0].recovery_state == "live" {
                "source-only".to_string()
            } else {
                "live".to_string()
            };
        let error = candidate
            .validate_baseline(&baseline)
            .expect_err("capability recovery readiness drift must be rejected");
        assert!(
            error
                .to_string()
                .contains("changed the frozen recovery state")
        );
    }

    #[test]
    fn builtin_manifest_anchors_exist_in_this_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        builtin_manifest()
            .unwrap()
            .validate_source_paths(&root)
            .expect("generated manifest paths should be present");
    }
}
