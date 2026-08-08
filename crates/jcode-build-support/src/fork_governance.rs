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

    pub fn with_digest(mut self) -> Result<Self> {
        self.sha256 = Some(self.digest()?);
        Ok(self)
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
        let active = self.active_by_id();
        let retired = self
            .retirements
            .iter()
            .map(|record| record.capability_id.as_str())
            .collect::<HashSet<_>>();
        for capability in &baseline.capabilities {
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
    manifest.validate()?;
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
    fn builtin_manifest_anchors_exist_in_this_checkout() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        builtin_manifest()
            .unwrap()
            .validate_source_paths(&root)
            .expect("generated manifest paths should be present");
    }
}
