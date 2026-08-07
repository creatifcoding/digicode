use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jcode_artifact_store::{AdmitBundleInput, ArtifactStore};
use jcode_metatool_runtime::{
    AGENTOS_PACKAGE, AGENTOS_VERSION, AgentOsExecutor, AgentOsRuntimeConfig, GUEST_ENGINE_FILE,
    JavaScriptExecutor, SIDECAR_SOURCE,
};
use jcode_metatool_types::{
    ExecutionEffect, ExecutionId, ExecutionLimits, ExecutionProfile, ExecutionRequest,
    MetaToolError,
};
use jcode_tasker_orchestration::{CandidateExecutionReport, CandidateLaneLaunchRequest};
use jcode_tasker_pi::{
    BatchOperation, CreateFeature, CreateTask, FeaturePlanFeature, FeaturePlanInput, NoteInput,
    PiTaskerStore, PlanTask, ProjectPartition, ResolveFeatureGate, UpdateFeature, UpdateTask,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use super::{Tool, ToolContext, ToolOutput};

const RUNTIME_DIR_ENV: &str = "JCODE_METATOOL_RUNTIME_DIR";
const NODE_BINARY_ENV: &str = "JCODE_METATOOL_NODE";
const SIDECAR_FILE: &str = "jcode-codemode-sidecar.mjs";
const GUEST_ENGINE_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../jcode-metatool-runtime/assets/guest-engine.mjs"
));
const GUIDE_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../jcode-metatool-runtime/assets/guide.json"
));
#[cfg(test)]
const TASKER_PARITY_MANIFEST_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/TASKER_PARITY_MANIFEST.json"
));
const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const TASKER_SNAPSHOT_LIMIT: usize = 500;
const TASKER_RECEIPT_TTL_SECONDS: u64 = 30 * 60;
const TASKER_RECEIPT_PREFIX: &str = "tpr_";
const ARTIFACT_CATALOG_LIMIT: usize = 200;
const ARTIFACT_MAX_TEXT_BYTES: usize = 1024 * 1024;
const ORDINARY_TASKER_RECONCILIATION_KINDS: &[&str] = &[
    "batch",
    "plan",
    "feature_plan",
    "create",
    "update",
    "set_state",
    "add_dependency",
    "add_note",
    "task_index",
    "create_feature",
    "feature_update",
    "resolve_feature_gate",
    "set_dependencies",
    "set_feature_dependencies",
    "link",
    "unlink",
];
const CONCURRENCY_LIFECYCLE_KINDS: &[&str] = &[
    "create_candidate_set",
    "register_candidate",
    "submit_candidate",
    "set_candidate_set_state",
    "record_round",
    "record_ballot",
    "prepare_promotion",
    "mark_promotion_ref_updated",
    "finalize_promotion",
    "abort_promotion",
    "rollback_promotion",
    "recover_promotion",
    "resume_promotion",
];
const CANDIDATE_LANE_COMPATIBILITY_KIND: &str = "execute_candidate_lanes";

static CANDIDATE_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

fn next_candidate_request_id() -> u64 {
    CANDIDATE_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateLaneHostRequest {
    pub id: u64,
    pub operation_id: String,
    pub session_id: String,
    pub working_dir: String,
    pub expected_snapshot_hash: String,
    pub receipt_id: String,
    pub proposal: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateLaneHostResponse {
    pub status: String,
    pub redacted: bool,
    pub report: Value,
}

#[async_trait]
pub(crate) trait CandidateLaneHost: Send + Sync {
    async fn execute_candidate_lanes(
        &self,
        request: CandidateLaneHostRequest,
        cancellation: CancellationToken,
    ) -> Result<CandidateLaneHostResponse>;
}

/// Socket-backed host callback used by live server registries. Keeping this
/// callback on the registry, rather than in ToolContext or the guest runtime,
/// preserves the static MetaTool/base-tool boundary while following the same
/// request/response transport as CommunicateTool.
pub(crate) struct SocketCandidateLaneHost;

impl SocketCandidateLaneHost {
    pub(crate) const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CandidateLaneHost for SocketCandidateLaneHost {
    async fn execute_candidate_lanes(
        &self,
        request: CandidateLaneHostRequest,
        cancellation: CancellationToken,
    ) -> Result<CandidateLaneHostResponse> {
        let wire_request = crate::protocol::Request::TaskerCandidateExecute {
            id: request.id,
            request: crate::protocol::TaskerCandidateExecutionRequest {
                operation_id: request.operation_id.clone(),
                session_id: request.session_id.clone(),
                working_dir: request.working_dir.clone(),
                expected_snapshot_hash: request.expected_snapshot_hash.clone(),
                receipt_id: request.receipt_id.clone(),
                proposal: request.proposal,
            },
        };
        let response = tokio::select! {
            response = super::communicate::send_request(wire_request) => response?,
            _ = cancellation.cancelled() => {
                let _ = super::communicate::send_request(
                    crate::protocol::Request::TaskerCandidateCancel {
                        id: next_candidate_request_id(),
                        session_id: request.session_id,
                        operation_id: request.operation_id,
                    },
                ).await;
                anyhow::bail!("candidate execution cancellation requested")
            }
        };
        match response {
            crate::protocol::ServerEvent::TaskerCandidateExecutionResponse {
                status,
                redacted,
                report,
                ..
            } => Ok(CandidateLaneHostResponse {
                status,
                redacted,
                report,
            }),
            crate::protocol::ServerEvent::Error { message, .. } => {
                anyhow::bail!("candidate execution request failed: {message}")
            }
            other => anyhow::bail!(
                "unexpected candidate execution response: {}",
                serde_json::to_string(&other).unwrap_or_else(|_| "unserializable".into())
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateLaneExecutionBlocked {
    HostExecutorUnavailable,
}

impl CandidateLaneExecutionBlocked {
    const fn code(self) -> &'static str {
        match self {
            Self::HostExecutorUnavailable => "host_executor_unavailable",
        }
    }
}

impl fmt::Display for CandidateLaneExecutionBlocked {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

impl std::error::Error for CandidateLaneExecutionBlocked {}

pub struct MetaTool {
    store_root_override: Option<PathBuf>,
    tasker_database_path_override: Option<PathBuf>,
    tasker_receipt_root_override: Option<PathBuf>,
    artifact_root_override: Option<PathBuf>,
    candidate_lane_host: Option<Arc<dyn CandidateLaneHost>>,
}

impl MetaTool {
    pub fn new() -> Self {
        Self {
            store_root_override: None,
            tasker_database_path_override: None,
            tasker_receipt_root_override: None,
            artifact_root_override: None,
            candidate_lane_host: None,
        }
    }

    pub(crate) fn with_candidate_lane_host(mut self, host: Arc<dyn CandidateLaneHost>) -> Self {
        self.candidate_lane_host = Some(host);
        self
    }

    #[cfg(test)]
    fn with_store_root(store_root: PathBuf) -> Self {
        Self {
            store_root_override: Some(store_root),
            tasker_database_path_override: None,
            tasker_receipt_root_override: None,
            artifact_root_override: None,
            candidate_lane_host: None,
        }
    }

    #[cfg(test)]
    fn with_store_and_tasker_roots(store_root: PathBuf, tasker_database_path: PathBuf) -> Self {
        let tasker_receipt_root = tasker_database_path.with_extension("receipts");
        Self {
            store_root_override: Some(store_root),
            tasker_database_path_override: Some(tasker_database_path),
            tasker_receipt_root_override: Some(tasker_receipt_root),
            artifact_root_override: None,
            candidate_lane_host: None,
        }
    }

    fn tasker_database_path(&self) -> PathBuf {
        self.tasker_database_path_override
            .clone()
            .unwrap_or_else(jcode_tasker_pi::default_db_path)
    }

    fn tasker_receipt_root(&self) -> Result<PathBuf> {
        self.tasker_receipt_root_override
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Ok(crate::storage::jcode_dir()?.join("tasker-plan-receipts")))
    }

    /// Workspace-scoped durable store directory: the guest's /data mount is
    /// backed by this host directory write-through.
    fn store_root(&self, ctx: &ToolContext) -> Result<PathBuf> {
        if let Some(root) = &self.store_root_override {
            return Ok(root.clone());
        }
        let workspace = ctx
            .working_dir
            .as_deref()
            .ok_or_else(|| anyhow!("mt evaluate requires a session working directory"))?;
        let canonical =
            std::fs::canonicalize(workspace).unwrap_or_else(|_| workspace.to_path_buf());
        let digest = {
            use sha2::{Digest, Sha256};
            format!(
                "{:x}",
                Sha256::digest(canonical.to_string_lossy().as_bytes())
            )
        };
        let name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace");
        Ok(crate::storage::jcode_dir()?
            .join("metatool/stores")
            .join(format!("{name}-{}", &digest[..12])))
    }

    fn runtime_dir() -> Result<PathBuf> {
        match std::env::var_os(RUNTIME_DIR_ENV) {
            Some(path) => Ok(PathBuf::from(path)),
            None => Ok(crate::storage::jcode_dir()?
                .join("runtimes/metatool-agentos")
                .join(AGENTOS_VERSION)),
        }
    }

    fn node_binary() -> Option<PathBuf> {
        if let Some(path) = std::env::var_os(NODE_BINARY_ENV) {
            let path = PathBuf::from(path);
            return path.is_file().then_some(path);
        }
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|directory| directory.join("node"))
                .find(|candidate| candidate.is_file())
        })
    }

    fn package_path(runtime_dir: &Path) -> PathBuf {
        runtime_dir.join("node_modules/@rivet-dev/agentos-core/package.json")
    }

    fn installed_package_version(runtime_dir: &Path) -> Option<String> {
        let package: Value =
            serde_json::from_slice(&std::fs::read(Self::package_path(runtime_dir)).ok()?).ok()?;
        package["version"].as_str().map(str::to_owned)
    }

    fn sidecar_path(runtime_dir: &Path) -> PathBuf {
        runtime_dir.join(SIDECAR_FILE)
    }

    fn status() -> Result<Value> {
        let runtime_dir = Self::runtime_dir()?;
        let node_binary = Self::node_binary();
        let sidecar_path = Self::sidecar_path(&runtime_dir);
        let installed_version = Self::installed_package_version(&runtime_dir);
        let package_verified = installed_version.as_deref() == Some(AGENTOS_VERSION);
        let available = node_binary.is_some() && package_verified;
        Ok(json!({
            "available": available,
            "experimental": true,
            "package": AGENTOS_PACKAGE,
            "version": AGENTOS_VERSION,
            "runtime_dir": runtime_dir,
            "node_binary": node_binary,
            "installed_version": installed_version,
            "package_verified": package_verified,
            "sidecar_present": sidecar_path.is_file(),
            "profiles": {
                "pure": "available when runtime is installed",
                "workspace-read": "blocked until the capability broker is implemented",
                "workspace-mutate": "blocked until explicit authority and provenance are implemented"
            },
            "setup": format!(
                "Install pinned {AGENTOS_PACKAGE}@{AGENTOS_VERSION} into the runtime directory or set {RUNTIME_DIR_ENV}; optionally set {NODE_BINARY_ENV} to an absolute Node.js binary."
            ),
            "security_gate": "Dependency reachability review and precise CPU-timeout classification remain open."
        }))
    }

    fn materialize_asset(runtime_dir: &Path, file_name: &str, source: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(runtime_dir).with_context(|| {
            format!(
                "create MetaTool runtime directory {}",
                runtime_dir.display()
            )
        })?;
        let path = runtime_dir.join(file_name);
        let expected = {
            use sha2::{Digest, Sha256};
            format!("{:x}", Sha256::digest(source.as_bytes()))
        };
        let current_matches = std::fs::read(&path)
            .ok()
            .map(|bytes| {
                use sha2::{Digest, Sha256};
                format!("{:x}", Sha256::digest(bytes)) == expected
            })
            .unwrap_or(false);
        if !current_matches {
            use std::io::Write;

            let temporary = runtime_dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .with_context(|| format!("create MetaTool asset {file_name}"))?;
            file.write_all(source.as_bytes())
                .with_context(|| format!("write MetaTool asset {file_name}"))?;
            file.sync_all()
                .with_context(|| format!("sync MetaTool asset {file_name}"))?;
            drop(file);
            std::fs::rename(&temporary, &path)
                .with_context(|| format!("publish MetaTool asset {file_name}"))?;
        }
        Ok(path)
    }

    fn materialize_sidecar(runtime_dir: &Path) -> Result<PathBuf> {
        Self::materialize_asset(runtime_dir, GUEST_ENGINE_FILE, GUEST_ENGINE_SOURCE)?;
        Self::materialize_asset(runtime_dir, SIDECAR_FILE, SIDECAR_SOURCE)
    }

    fn executor() -> Result<AgentOsExecutor> {
        let runtime_dir = Self::runtime_dir()?;
        let installed_version = Self::installed_package_version(&runtime_dir);
        if installed_version.as_deref() != Some(AGENTOS_VERSION) {
            return Err(anyhow!(MetaToolError::RuntimeUnavailable {
                message: format!(
                    "pinned {AGENTOS_PACKAGE}@{AGENTOS_VERSION} is not installed in {}; found {}",
                    runtime_dir.display(),
                    installed_version.as_deref().unwrap_or("nothing")
                ),
            }));
        }
        let node_binary = Self::node_binary().ok_or_else(|| {
            anyhow!(MetaToolError::RuntimeUnavailable {
                message: format!(
                    "Node.js was not found; set {NODE_BINARY_ENV} to an absolute binary path"
                ),
            })
        })?;
        let sidecar_path = Self::materialize_sidecar(&runtime_dir)?;
        AgentOsExecutor::new(AgentOsRuntimeConfig {
            node_binary,
            runtime_dir,
            sidecar_path,
            expected_sidecar_sha256: AgentOsRuntimeConfig::expected_sidecar_sha256(),
        })
        .map_err(anyhow::Error::new)
    }

    fn artifact_root(&self) -> Result<PathBuf> {
        self.artifact_root_override
            .clone()
            .map(Ok)
            .unwrap_or_else(|| Ok(crate::storage::jcode_dir()?.join("artifacts")))
    }

    fn artifact_store(&self) -> Result<ArtifactStore> {
        let root = self.artifact_root()?;
        ArtifactStore::open_migrate(root.join("artifacts.sqlite3"), root.join("assets"))
            .map_err(anyhow::Error::new)
    }

    fn tasker_store(&self, ctx: &ToolContext) -> Result<PiTaskerStore> {
        let root = ctx
            .working_dir
            .as_deref()
            .ok_or_else(|| anyhow!("Tasker codemode requires a session working directory"))?;
        let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
        PiTaskerStore::open(ProjectPartition::with_db_path(
            self.tasker_database_path(),
            canonical.to_string_lossy().into_owned(),
        ))
        .map_err(anyhow::Error::new)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct MetaToolInput {
    #[serde(default = "default_action")]
    action: MetaToolAction,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    inputs: Value,
    #[serde(default = "default_profile")]
    profile: ExecutionProfile,
    #[serde(default)]
    tasker_mode: TaskerMode,
    #[serde(default)]
    tasker_receipt: Option<String>,
    #[serde(default)]
    tasker_project_id: Option<String>,
    #[serde(default)]
    artifact_mode: ArtifactMode,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TaskerMode {
    #[default]
    Off,
    Plan,
    Apply,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ArtifactMode {
    #[default]
    Off,
    Apply,
}

#[derive(Debug, Deserialize)]
struct TaskerEffectInput {
    kind: String,
    payload: Value,
    mode: TaskerMode,
    expected_snapshot_hash: Option<String>,
    #[serde(default)]
    project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeCreateTaskInput {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    feature_id: Option<String>,
    #[serde(default)]
    indexes: Option<Value>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    notes: Vec<NoteInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeUpdateTaskInput {
    task_id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<Option<String>>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    feature_id: Option<Option<String>>,
    #[serde(default)]
    indexes: Option<Value>,
    #[serde(default)]
    depends_on: Option<Vec<String>>,
    #[serde(default)]
    clear_dependencies: bool,
    #[serde(default)]
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeCreateFeatureInput {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    parent_feature_id: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    tags: Option<Value>,
    #[serde(default)]
    brief: Option<String>,
    #[serde(default)]
    acceptance: Option<Value>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    gates: Option<Value>,
    #[serde(default)]
    indexes: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeUpdateFeatureInput {
    feature_id: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<Option<String>>,
    #[serde(default)]
    parent_feature_id: Option<Option<String>>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    tags: Option<Value>,
    #[serde(default)]
    brief: Option<Option<String>>,
    #[serde(default)]
    acceptance: Option<Value>,
    #[serde(default)]
    owner: Option<Option<String>>,
    #[serde(default)]
    gates: Option<Value>,
    #[serde(default)]
    indexes: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeTaskIndexInput {
    task_id: String,
    #[serde(default)]
    index_set: Option<Value>,
    #[serde(default)]
    index_add: Option<Value>,
    #[serde(default)]
    index_remove: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeAddDependencyInput {
    task_id: String,
    depends_on_task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeAddNoteInput {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    feature_id: Option<String>,
    #[serde(default)]
    category: Option<String>,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeDependenciesInput {
    task_id: String,
    #[serde(default)]
    depends_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeFeatureDependenciesInput {
    feature_id: String,
    #[serde(default)]
    depends_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodemodeLinkInput {
    task_id: String,
    #[serde(default)]
    feature_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactBundleInput {
    key: String,
    title: String,
    source: String,
    rendered: String,
    #[serde(default)]
    annotation: Option<String>,
    #[serde(default)]
    template_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TaskerPlanReceipt {
    version: u8,
    id: String,
    project_root: String,
    list_id: String,
    snapshot_hash: String,
    change_digest: String,
    #[serde(default)]
    concurrency_project_id: Option<String>,
    issued_at: u64,
    expires_at: u64,
}

fn unix_timestamp() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_secs())
}

fn canonical_json(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| (key.clone(), canonical_json(value)))
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(canonical_json).collect()),
        _ => value.clone(),
    }
}

fn tasker_change_digest(input: &TaskerEffectInput) -> Result<String> {
    let canonical = canonical_json(&json!({
        "kind": input.kind,
        "payload": input.payload,
        "project_id": input.project_id,
    }));
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical).context("serialize Tasker change set")?)
    ))
}

fn validate_receipt_id(id: &str) -> Result<()> {
    if !id.starts_with(TASKER_RECEIPT_PREFIX)
        || id.len() > 64
        || !id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(anyhow!("invalid Tasker plan receipt id"));
    }
    Ok(())
}

fn receipt_path(root: &Path, id: &str) -> Result<PathBuf> {
    validate_receipt_id(id)?;
    Ok(root.join(format!("{id}.json")))
}

fn issue_tasker_receipt(
    root: &Path,
    store: &PiTaskerStore,
    snapshot_hash: &str,
    input: &TaskerEffectInput,
) -> Result<TaskerPlanReceipt> {
    std::fs::create_dir_all(root).context("create Tasker plan receipt directory")?;
    let issued_at = unix_timestamp()?;
    let partition = store.partition();
    let receipt = TaskerPlanReceipt {
        version: 1,
        id: format!("{TASKER_RECEIPT_PREFIX}{}", uuid::Uuid::now_v7()),
        project_root: partition.project_root.clone(),
        list_id: partition.list_id.clone(),
        snapshot_hash: snapshot_hash.to_owned(),
        change_digest: tasker_change_digest(input)?,
        concurrency_project_id: input.project_id.clone(),
        issued_at,
        expires_at: issued_at + TASKER_RECEIPT_TTL_SECONDS,
    };
    let path = receipt_path(root, &receipt.id)?;
    let temporary = root.join(format!(".{}.{}.tmp", receipt.id, uuid::Uuid::now_v7()));
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&receipt).context("serialize Tasker plan receipt")?,
    )
    .context("write Tasker plan receipt")?;
    std::fs::rename(&temporary, &path).context("publish Tasker plan receipt")?;
    Ok(receipt)
}

fn verify_tasker_receipt(
    root: &Path,
    id: &str,
    store: &PiTaskerStore,
    snapshot_hash: &str,
    input: &TaskerEffectInput,
) -> Result<TaskerPlanReceipt> {
    let path = receipt_path(root, id)?;
    let receipt: TaskerPlanReceipt =
        serde_json::from_slice(&std::fs::read(path).context("Tasker plan receipt not found")?)
            .context("parse Tasker plan receipt")?;
    let partition = store.partition();
    if receipt.version != 1
        || receipt.id != id
        || receipt.project_root != partition.project_root
        || receipt.list_id != partition.list_id
    {
        return Err(anyhow!("Tasker plan receipt scope mismatch"));
    }
    if receipt.expires_at < unix_timestamp()? {
        return Err(anyhow!("Tasker plan receipt expired"));
    }
    if receipt.snapshot_hash != snapshot_hash {
        return Err(anyhow!("Tasker plan receipt snapshot mismatch"));
    }
    if receipt.concurrency_project_id != input.project_id {
        return Err(anyhow!("Tasker plan receipt concurrency project mismatch"));
    }
    if receipt.change_digest != tasker_change_digest(input)? {
        return Err(anyhow!("Tasker plan receipt change-set mismatch"));
    }
    Ok(receipt)
}

fn prepare_candidate_lane_apply(
    store: &mut PiTaskerStore,
    effects: &[ExecutionEffect],
    receipt_root: &Path,
    requested_receipt: Option<&str>,
) -> Result<(CandidateLaneLaunchRequest, TaskerPlanReceipt, String)> {
    if effects.len() != 1 {
        return Err(anyhow!(
            "Tasker candidate execution requires exactly one effect"
        ));
    }
    let effect = &effects[0];
    if effect.capability != "tasker" || effect.operation != "reconcile" {
        return Err(anyhow!(
            "unsupported MetaTool capability effect {}.{}",
            effect.capability,
            effect.operation
        ));
    }
    let mut input: TaskerEffectInput = serde_json::from_value(effect.input.clone())?;
    if input.kind != CANDIDATE_LANE_COMPATIBILITY_KIND || input.mode != TaskerMode::Apply {
        return Err(anyhow!(
            "candidate host callback received a non-apply effect"
        ));
    }

    // Candidate execution is always bound to the canonical project/list for
    // this working directory. A guest-supplied project id is never used.
    let canonical_project_id = store.partition().list_id.clone();
    input.project_id = Some(canonical_project_id.clone());
    let (_, canonical_snapshot_hash) = tasker_snapshot_for_project(store, &canonical_project_id)?;
    if input.expected_snapshot_hash.as_deref() != Some(canonical_snapshot_hash.as_str()) {
        return Err(anyhow!(
            "Tasker candidate snapshot identity does not match canonical host state"
        ));
    }
    let receipt_id = requested_receipt
        .ok_or_else(|| anyhow!("tasker_receipt from a successful plan is required for apply"))?;
    let receipt = verify_tasker_receipt(
        receipt_root,
        receipt_id,
        store,
        &canonical_snapshot_hash,
        &input,
    )?;
    let proposal: CandidateLaneLaunchRequest = serde_json::from_value(input.payload)?;
    proposal
        .validate()
        .map_err(|error| anyhow!("candidate lane request rejected: {error}"))?;
    Ok((proposal, receipt, canonical_snapshot_hash))
}

fn validate_candidate_host_response(
    response: &CandidateLaneHostResponse,
) -> Result<CandidateExecutionReport> {
    if response.status != "completed" || !response.redacted {
        return Err(anyhow!(
            "candidate host did not return an observed redacted completion"
        ));
    }
    let report: CandidateExecutionReport = serde_json::from_value(response.report.clone())
        .context("parse structured candidate execution report")?;
    if report.outcomes.is_empty()
        || report
            .outcomes
            .iter()
            .any(|outcome| !outcome.state.is_terminal())
    {
        return Err(anyhow!(
            "candidate host report did not terminally account for every lane"
        ));
    }
    if report.submitted_count() == 0 {
        return Err(anyhow!(
            "candidate host report contains no submitted lane; metadata-only success is forbidden"
        ));
    }
    Ok(report)
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum MetaToolAction {
    Status,
    #[default]
    Evaluate,
    Guide,
}

fn default_action() -> MetaToolAction {
    MetaToolAction::Evaluate
}

fn default_profile() -> ExecutionProfile {
    ExecutionProfile::Pure
}

#[cfg(test)]
fn tasker_snapshot(store: &PiTaskerStore) -> Result<(Value, String)> {
    let project_id = store.partition().list_id.clone();
    tasker_snapshot_for_project(store, &project_id)
}

pub(crate) fn tasker_snapshot_for_project(
    store: &PiTaskerStore,
    concurrency_project_id: &str,
) -> Result<(Value, String)> {
    let snapshot = store.snapshot()?;
    let ready = store.ready_tasks()?;
    let concurrency = store.open_concurrency_store()?;
    let concurrency_projection =
        concurrency.project(concurrency_project_id, TASKER_SNAPSHOT_LIMIT)?;
    let concurrency_revision = concurrency.current_revision(concurrency_project_id)?;
    let concurrency_schema_version = concurrency.schema_version()?;
    let canonical = json!({
        "list_meta": snapshot.list_meta,
        "tasks": snapshot.tasks,
        "dependencies": snapshot.dependencies,
        "features": snapshot.features,
        "feature_dependencies": snapshot.feature_dependencies,
        "task_notes": snapshot.task_notes,
        "feature_notes": snapshot.feature_notes,
        "concurrency": {
            "project_id": concurrency_project_id,
            "schema_version": concurrency_schema_version,
            "revision": concurrency_revision,
            "projection": concurrency_projection.clone(),
        },
    });
    let hash = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical).context("serialize Tasker snapshot")?)
    );
    let tasks = canonical["tasks"].as_array().cloned().unwrap_or_default();
    let features = canonical["features"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let dependencies = canonical["dependencies"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let feature_dependencies = canonical["feature_dependencies"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let task_notes = canonical["task_notes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let feature_notes = canonical["feature_notes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let task_count = tasks.len();
    let feature_count = features.len();
    let dependency_count = dependencies.len();
    let feature_dependency_count = feature_dependencies.len();
    let task_note_count = task_notes.len();
    let feature_note_count = feature_notes.len();
    let concurrency_truncated = concurrency_projection["truncated"] == true;
    let projection_limit = TASKER_SNAPSHOT_LIMIT;
    let projections = json!({
        "taskGraph": store.task_graph_projection(projection_limit)?,
        "taskStructure": store.task_structure_projection(projection_limit)?,
        "topologySummary": store.topology_summary_projection(projection_limit)?,
        "topologyAnomalies": store.topology_anomalies_projection(projection_limit)?,
        "topologyFrontier": store.topology_frontier_projection(projection_limit)?,
        "featureTree": store.feature_tree_projection(2, projection_limit, false)?,
    });
    let candidate_sets = concurrency_projection["candidateSets"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let supported_policies = ["exclusive", "speculative", "ensemble"]
        .into_iter()
        .map(Value::from)
        .collect::<Vec<_>>();
    let partition = store.partition();
    let bounded = json!({
        "partition": {
            "database_path": partition.db_path,
            "project_root": partition.project_root,
            "list_id": partition.list_id,
        },
        "schema_fingerprint": store.schema_fingerprint()?,
        "counts": {
            "tasks": task_count,
            "features": feature_count,
            "dependencies": dependency_count,
            "feature_dependencies": feature_dependency_count,
            "task_notes": task_note_count,
            "feature_notes": feature_note_count,
            "ready": ready.len(),
            "candidate_sets": concurrency_projection["counts"]["candidateSets"],
            "candidates": concurrency_projection["counts"]["candidates"],
            "rounds": concurrency_projection["counts"]["adjudicationRounds"],
            "ballots": concurrency_projection["counts"]["adjudicationBallots"],
            "promotions": concurrency_projection["counts"]["promotionIntents"],
        },
        "tasks": tasks.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "features": features.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "dependencies": dependencies.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "featureDependencies": feature_dependencies.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "taskNotes": task_notes.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "featureNotes": feature_notes.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "ready": ready.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "projections": projections,
        "concurrency": {
            "projectId": concurrency_project_id,
            "schemaVersion": concurrency_schema_version,
            "revision": concurrency_revision,
            "projection": concurrency_projection,
            "candidateSets": candidate_sets,
            "policy": {
                "supported": supported_policies,
                "agentSpawning": "host_owned_bounded_headless_inline",
                "candidateLane": {
                    "maxLanes": 8,
                    "maxLaneTimeoutMs": 1800000,
                    "spawnMode": "headless_inline",
                    "guestAuthority": "declarative_effect_only",
                    "execution": "foundation_ready_apply_blocked_host_executor_unavailable"
                },
                "promotion": "single_writer"
            }
        },
        "lifecycle": {
            "policies": ["exclusive", "speculative", "ensemble"],
            "agentSpawning": "host_owned_bounded_headless_inline",
            "candidateAuthorship": "host_derived_submission_contract",
            "candidateExecution": "foundation_ready_apply_blocked_host_executor_unavailable",
            "promotion": "single_writer"
        },
        "truncated": task_count > TASKER_SNAPSHOT_LIMIT
            || feature_count > TASKER_SNAPSHOT_LIMIT
            || dependency_count > TASKER_SNAPSHOT_LIMIT
            || feature_dependency_count > TASKER_SNAPSHOT_LIMIT
            || task_note_count > TASKER_SNAPSHOT_LIMIT
            || feature_note_count > TASKER_SNAPSHOT_LIMIT
            || concurrency_truncated,
    });
    Ok((bounded, hash))
}

fn reconcile_tasker_effects(
    store: &mut PiTaskerStore,
    effects: &[ExecutionEffect],
    requested_mode: TaskerMode,
    initial_snapshot_hash: &str,
    receipt_root: &Path,
    requested_receipt: Option<&str>,
) -> Result<Value> {
    if effects.is_empty() {
        return Ok(json!({
            "mode": requested_mode,
            "status": "read_only",
            "effect_count": 0,
            "snapshot_hash": initial_snapshot_hash,
        }));
    }
    if effects.len() != 1 {
        return Err(anyhow!(
            "Tasker codemode requires exactly one atomic reconciliation; received {} effects",
            effects.len()
        ));
    }
    let effect = &effects[0];
    if effect.capability != "tasker" || effect.operation != "reconcile" {
        return Err(anyhow!(
            "unsupported MetaTool capability effect {}.{}",
            effect.capability,
            effect.operation
        ));
    }
    let mut input: TaskerEffectInput = serde_json::from_value(effect.input.clone())?;
    input.project_id = Some(
        input
            .project_id
            .clone()
            .unwrap_or_else(|| store.partition().list_id.clone()),
    );
    if input.mode != requested_mode {
        return Err(anyhow!(
            "Tasker capability mode {:?} does not match requested mode {:?}",
            input.mode,
            requested_mode
        ));
    }
    if input.expected_snapshot_hash.as_deref() != Some(initial_snapshot_hash) {
        return Err(anyhow!("Tasker capability snapshot identity mismatch"));
    }
    if is_concurrency_kind(&input.kind) {
        return Err(concurrency_lifecycle_rejected(&input.kind));
    }

    if requested_mode == TaskerMode::Plan {
        if requested_receipt.is_some() {
            return Err(anyhow!("tasker_receipt is valid only for apply mode"));
        }
        let mut simulation = store.fork_in_memory()?;
        let simulated = if input.kind == CANDIDATE_LANE_COMPATIBILITY_KIND {
            candidate_lane_blocked_descriptor(&input.payload)?
        } else {
            execute_tasker_reconciliation(&mut simulation, &input)?
        };
        let simulated = normalize_tasker_simulation(&input, &simulated);
        let receipt = issue_tasker_receipt(receipt_root, store, initial_snapshot_hash, &input)?;
        return Ok(json!({
            "mode": requested_mode,
            "status": "simulated",
            "validated": true,
            "kind": input.kind,
            "snapshot_hash": initial_snapshot_hash,
            "change_set": input.payload,
            "simulation": simulated,
            "receipt": receipt,
        }));
    }
    if requested_mode != TaskerMode::Apply {
        return Err(anyhow!("Tasker effects require tasker_mode plan or apply"));
    }

    let receipt_id = requested_receipt
        .ok_or_else(|| anyhow!("tasker_receipt from a successful plan is required for apply"))?;
    let receipt = verify_tasker_receipt(
        receipt_root,
        receipt_id,
        store,
        initial_snapshot_hash,
        &input,
    )?;

    let project_id = input
        .project_id
        .as_deref()
        .unwrap_or(&store.partition().list_id);
    let (_, current_hash) = tasker_snapshot_for_project(store, project_id)?;
    if current_hash != initial_snapshot_hash {
        return Err(anyhow!(
            "Tasker state changed during codemode evaluation; rerun to reconcile against the current snapshot"
        ));
    }

    if input.kind == CANDIDATE_LANE_COMPATIBILITY_KIND {
        let _ = candidate_lane_blocked_descriptor(&input.payload)?;
        return Err(anyhow::Error::new(
            CandidateLaneExecutionBlocked::HostExecutorUnavailable,
        ));
    }

    let applied = execute_tasker_reconciliation(store, &input)?;
    Ok(json!({
        "mode": requested_mode,
        "status": "applied",
        "kind": input.kind,
        "snapshot_hash": initial_snapshot_hash,
        "receipt": {
            "id": receipt.id,
            "change_digest": receipt.change_digest,
        },
        "result": applied,
    }))
}

fn resolve_task_reference(store: &PiTaskerStore, reference: &str) -> Result<String> {
    store
        .resolve_task_id(reference)?
        .ok_or_else(|| anyhow!("Task reference not found: {reference}"))
}

fn resolve_feature_reference(store: &PiTaskerStore, reference: &str) -> Result<String> {
    store
        .resolve_feature_id(reference)?
        .ok_or_else(|| anyhow!("Feature reference not found: {reference}"))
}

fn payload_field<'a>(payload: &'a Value, name: &str) -> Result<&'a Value> {
    payload
        .get(name)
        .ok_or_else(|| anyhow!("Tasker reconciliation payload requires {name}"))
}

fn payload_string(payload: &Value, name: &str) -> Result<String> {
    payload_field(payload, name)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| anyhow!("Tasker reconciliation payload field {name} must be a string"))
}

fn payload_usize(payload: &Value, name: &str) -> Result<usize> {
    let value = payload_field(payload, name)?
        .as_u64()
        .ok_or_else(|| anyhow!("Tasker reconciliation payload field {name} must be an integer"))?;
    usize::try_from(value).map_err(|_| anyhow!("Tasker reconciliation field {name} is too large"))
}

fn is_concurrency_kind(kind: &str) -> bool {
    CONCURRENCY_LIFECYCLE_KINDS.contains(&kind)
}

fn concurrency_lifecycle_rejected(kind: &str) -> anyhow::Error {
    anyhow!("Tasker generic ordinary reconcile rejects concurrency lifecycle kind: {kind}")
}

fn execute_tasker_reconciliation(
    store: &mut PiTaskerStore,
    input: &TaskerEffectInput,
) -> Result<Value> {
    if is_concurrency_kind(&input.kind) {
        return Err(concurrency_lifecycle_rejected(&input.kind));
    }
    if !ORDINARY_TASKER_RECONCILIATION_KINDS.contains(&input.kind.as_str())
        && input.kind != CANDIDATE_LANE_COMPATIBILITY_KIND
    {
        return Err(anyhow!(
            "unsupported Tasker reconciliation kind: {}",
            input.kind
        ));
    }
    match input.kind.as_str() {
        "batch" => {
            let operations: Vec<BatchOperation> =
                serde_json::from_value(input.payload.get("operations").cloned().ok_or_else(
                    || anyhow!("Tasker batch reconciliation requires payload.operations"),
                )?)?;
            Ok(serde_json::to_value(store.batch_execute(operations)?)?)
        }
        "plan" => {
            let tasks: Vec<PlanTask> =
                serde_json::from_value(input.payload.get("tasks").cloned().ok_or_else(|| {
                    anyhow!("Tasker plan reconciliation requires payload.tasks")
                })?)?;
            Ok(serde_json::to_value(store.plan_import(tasks)?)?)
        }
        "feature_plan" => {
            let feature: FeaturePlanFeature =
                serde_json::from_value(input.payload.get("feature").cloned().ok_or_else(
                    || anyhow!("Tasker feature-plan reconciliation requires payload.feature"),
                )?)?;
            Ok(serde_json::to_value(
                store.feature_plan_import(FeaturePlanInput { feature })?,
            )?)
        }
        "create" => {
            let input: CodemodeCreateTaskInput = serde_json::from_value(input.payload.clone())?;
            let feature_id = input
                .feature_id
                .as_deref()
                .map(|reference| resolve_feature_reference(store, reference))
                .transpose()?;
            let depends_on = input
                .depends_on
                .iter()
                .map(|reference| resolve_task_reference(store, reference))
                .collect::<Result<Vec<_>>>()?;
            let task = store.create_task(CreateTask {
                title: input.title,
                description: input.description,
                state: input.state,
                feature_id,
                indexes: input.indexes,
                depends_on,
                notes: input.notes,
            })?;
            Ok(json!({"task": task}))
        }
        "update" | "set_state" => {
            let is_set_state = input.kind == "set_state";
            let input: CodemodeUpdateTaskInput = serde_json::from_value(input.payload.clone())?;
            if is_set_state && input.state.is_none() {
                return Err(anyhow!("set_state reconciliation requires state"));
            }
            let task_id = resolve_task_reference(store, &input.task_id)?;
            let feature_id = input
                .feature_id
                .map(|value| {
                    value
                        .as_deref()
                        .map(|reference| resolve_feature_reference(store, reference))
                        .transpose()
                })
                .transpose()?;
            let depends_on = input
                .depends_on
                .map(|references| {
                    references
                        .iter()
                        .map(|reference| resolve_task_reference(store, reference))
                        .collect::<Result<Vec<_>>>()
                })
                .transpose()?;
            let task = store.update_task(
                &task_id,
                UpdateTask {
                    title: input.title,
                    description: input.description,
                    state: input.state,
                    feature_id,
                    indexes: input.indexes,
                    depends_on,
                    clear_dependencies: input.clear_dependencies,
                    active: input.active,
                },
            )?;
            Ok(json!({"task": task}))
        }
        "add_dependency" => {
            let input: CodemodeAddDependencyInput = serde_json::from_value(input.payload.clone())?;
            let task_id = resolve_task_reference(store, &input.task_id)?;
            let depends_on = resolve_task_reference(store, &input.depends_on_task_id)?;
            let dependencies = store
                .list_dependencies()?
                .into_iter()
                .filter(|dependency| dependency.task_id == task_id)
                .map(|dependency| dependency.depends_on_id)
                .chain(std::iter::once(depends_on))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            Ok(json!({"dependencies": store.set_dependencies(&task_id, &dependencies)?}))
        }
        "add_note" => {
            let input: CodemodeAddNoteInput = serde_json::from_value(input.payload.clone())?;
            match (input.task_id, input.feature_id) {
                (Some(task_id), None) => {
                    let task_id = resolve_task_reference(store, &task_id)?;
                    let note = store.append_task_note(
                        &task_id,
                        input.category.as_deref(),
                        &input.content,
                    )?;
                    Ok(json!({"note": note}))
                }
                (None, Some(feature_id)) => {
                    let feature_id = resolve_feature_reference(store, &feature_id)?;
                    let note = store.append_feature_note(
                        &feature_id,
                        input.category.as_deref(),
                        &input.content,
                    )?;
                    Ok(json!({"note": note}))
                }
                _ => Err(anyhow!(
                    "add_note requires exactly one of taskId or featureId"
                )),
            }
        }
        "task_index" => {
            let input: CodemodeTaskIndexInput = serde_json::from_value(input.payload.clone())?;
            let task_id = resolve_task_reference(store, &input.task_id)?;
            let task = if let Some(indexes) = input.index_set {
                store.set_task_indexes(&task_id, indexes)?
            } else {
                if !input.index_remove.is_empty() {
                    store.remove_task_indexes(&task_id, &input.index_remove)?;
                }
                if let Some(add) = input.index_add {
                    store.add_task_indexes(&task_id, add)?
                } else {
                    store
                        .list_tasks(None)?
                        .into_iter()
                        .find(|task| task.id == task_id)
                        .ok_or_else(|| anyhow!("task disappeared while updating indexes"))?
                }
            };
            let indexes = task.indexes.clone();
            Ok(json!({"task": task, "indexes": indexes}))
        }
        "create_feature" => {
            let input: CodemodeCreateFeatureInput = serde_json::from_value(input.payload.clone())?;
            let parent_feature_id = input
                .parent_feature_id
                .as_deref()
                .map(|reference| resolve_feature_reference(store, reference))
                .transpose()?;
            let feature = store.create_feature(CreateFeature {
                title: input.title,
                description: input.description,
                parent_feature_id,
                state: input.state,
                priority: input.priority,
                tags: input.tags.unwrap_or_else(|| json!([])),
                brief: input.brief,
                acceptance: input.acceptance.unwrap_or_else(|| json!([])),
                owner: input.owner,
                gates: input.gates.unwrap_or_else(|| json!([])),
                indexes: input.indexes.unwrap_or_else(|| json!([])),
            })?;
            Ok(json!({"feature": feature}))
        }
        "feature_update" => {
            let input: CodemodeUpdateFeatureInput = serde_json::from_value(input.payload.clone())?;
            let feature_id = resolve_feature_reference(store, &input.feature_id)?;
            let parent_feature_id = input
                .parent_feature_id
                .map(|value| {
                    value
                        .as_deref()
                        .map(|reference| resolve_feature_reference(store, reference))
                        .transpose()
                })
                .transpose()?;
            let feature = store.update_feature(
                &feature_id,
                UpdateFeature {
                    title: input.title,
                    description: input.description,
                    parent_feature_id,
                    state: input.state,
                    priority: input.priority,
                    tags: input.tags,
                    brief: input.brief,
                    acceptance: input.acceptance,
                    owner: input.owner,
                    gates: input.gates,
                    indexes: input.indexes,
                },
            )?;
            Ok(json!({"feature": feature}))
        }
        "resolve_feature_gate" => {
            let feature_id =
                resolve_feature_reference(store, &payload_string(&input.payload, "featureId")?)?;
            let gate_index = payload_usize(&input.payload, "gateIndex")?;
            let resolution: ResolveFeatureGate = serde_json::from_value(input.payload.clone())?;
            Ok(json!({
                "featureId": feature_id,
                "gates": store.resolve_feature_gate(&feature_id, gate_index, resolution)?
            }))
        }
        "set_dependencies" => {
            let input: CodemodeDependenciesInput = serde_json::from_value(input.payload.clone())?;
            let task_id = resolve_task_reference(store, &input.task_id)?;
            let dependencies = input
                .depends_on
                .iter()
                .map(|reference| resolve_task_reference(store, reference))
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({"dependencies": store.set_dependencies(&task_id, &dependencies)?}))
        }
        "set_feature_dependencies" => {
            let input: CodemodeFeatureDependenciesInput =
                serde_json::from_value(input.payload.clone())?;
            let feature_id = resolve_feature_reference(store, &input.feature_id)?;
            let dependencies = input
                .depends_on
                .iter()
                .map(|reference| resolve_feature_reference(store, reference))
                .collect::<Result<Vec<_>>>()?;
            Ok(json!({
                "dependencies": store.set_feature_dependencies(&feature_id, &dependencies)?
            }))
        }
        "link" => {
            let input: CodemodeLinkInput = serde_json::from_value(input.payload.clone())?;
            let task_id = resolve_task_reference(store, &input.task_id)?;
            let feature_id = input
                .feature_id
                .as_deref()
                .ok_or_else(|| anyhow!("link reconciliation requires featureId"))
                .and_then(|reference| resolve_feature_reference(store, reference))?;
            store.link_task(&task_id, &feature_id)?;
            Ok(json!({"task": store.list_tasks(None)?.into_iter().find(|task| task.id == task_id)}))
        }
        "unlink" => {
            let task_id =
                resolve_task_reference(store, &payload_string(&input.payload, "taskId")?)?;
            store.unlink_task(&task_id)?;
            Ok(json!({"task": store.list_tasks(None)?.into_iter().find(|task| task.id == task_id)}))
        }
        CANDIDATE_LANE_COMPATIBILITY_KIND => {
            let _ = candidate_lane_blocked_descriptor(&input.payload)?;
            Err(anyhow::Error::new(
                CandidateLaneExecutionBlocked::HostExecutorUnavailable,
            ))
        }
        other => Err(anyhow!("unsupported Tasker reconciliation kind: {other}")),
    }
}

fn candidate_lane_blocked_descriptor(payload: &Value) -> Result<Value> {
    let request: CandidateLaneLaunchRequest = serde_json::from_value(payload.clone())?;
    let limits = request
        .validate()
        .map_err(|error| anyhow!("candidate lane request rejected: {error}"))?;
    Ok(json!({
        "status": "blocked",
        "blockedReason": CandidateLaneExecutionBlocked::HostExecutorUnavailable.code(),
        "execution": "host_owned_bounded_headless_inline",
        "guestAuthority": "declarative_effect_only",
        "laneCount": request.lane_count,
        "limits": limits,
        "acceptanceDigest": request.acceptance.digest(),
        "apply": "blocked_until_server_installs_host_executor",
    }))
}

fn normalize_tasker_simulation(input: &TaskerEffectInput, result: &Value) -> Value {
    match input.kind.as_str() {
        "batch" => json!({
            "created": result["created"],
            "updated": result["updated"],
            "operations": result["operations"].as_array().map(|operations| {
                operations.iter().map(|operation| json!({
                    "op": operation["op"],
                    "key": operation["key"],
                    "title": operation["task"]["title"],
                    "state": operation["task"]["state"],
                })).collect::<Vec<_>>()
            }).unwrap_or_default(),
        }),
        "plan" => json!({
            "task_count": result["taskCount"],
            "dependency_count": result["dependencyCount"],
            "tasks": input.payload["tasks"].as_array().map(|tasks| {
                tasks.iter().map(|task| json!({
                    "key": task["key"],
                    "title": task["title"],
                    "state": task.get("state").cloned().unwrap_or(Value::Null),
                    "after": task.get("after").cloned().unwrap_or_else(|| json!([])),
                })).collect::<Vec<_>>()
            }).unwrap_or_default(),
        }),
        "feature_plan" => json!({
            "feature_count": result["featureCount"],
            "task_count": result["taskCount"],
            "dependency_count": result["dependencyCount"],
            "feature": summarize_feature_plan(&input.payload["feature"]),
        }),
        "create" => json!({
            "task": stable_task(&result["task"], false),
        }),
        "update" | "set_state" | "link" | "unlink" => json!({
            "task": stable_task(&result["task"], true),
        }),
        "add_dependency" | "set_dependencies" | "set_feature_dependencies" => json!({
            "dependencies": result["dependencies"].clone(),
        }),
        "add_note" => json!({
            "note": {
                "taskId": result["note"]["taskId"],
                "featureId": result["note"]["featureId"],
                "category": result["note"]["category"],
                "content": result["note"]["content"],
            },
        }),
        "task_index" => json!({
            "indexes": result["indexes"].clone(),
        }),
        "create_feature" => json!({
            "feature": stable_feature(&result["feature"], false),
        }),
        "feature_update" => json!({
            "feature": stable_feature(&result["feature"], true),
        }),
        "resolve_feature_gate" => json!({
            "featureId": result["featureId"],
            "gates": result["gates"].clone(),
        }),
        CANDIDATE_LANE_COMPATIBILITY_KIND => result.clone(),
        kind if is_concurrency_kind(kind) => json!({
            "id": result["id"],
            "revision": result["revision"],
            "action": kind,
        }),
        _ => Value::Null,
    }
}

fn stable_task(task: &Value, include_id: bool) -> Value {
    let mut value = json!({
        "title": task["title"],
        "description": task["description"],
        "state": task["state"],
        "featureId": task["featureId"],
        "indexes": task["indexes"],
    });
    if include_id {
        value["id"] = task["id"].clone();
    }
    value
}

fn stable_feature(feature: &Value, include_id: bool) -> Value {
    let mut value = json!({
        "title": feature["title"],
        "description": feature["description"],
        "state": feature["state"],
        "priority": feature["priority"],
        "parentFeatureId": feature["parentFeatureId"],
        "tags": feature["tags"],
        "acceptance": feature["acceptance"],
        "gates": feature["gates"],
    });
    if include_id {
        value["id"] = feature["id"].clone();
    }
    value
}

fn summarize_feature_plan(feature: &Value) -> Value {
    json!({
        "key": feature["key"],
        "title": feature["title"],
        "tasks": feature["tasks"].as_array().map(|tasks| {
            tasks.iter().map(|task| json!({
                "key": task["key"],
                "title": task["title"],
                "state": task.get("state").cloned().unwrap_or(Value::Null),
                "after": task.get("after").cloned().unwrap_or_else(|| json!([])),
            })).collect::<Vec<_>>()
        }).unwrap_or_default(),
        "children": feature["children"].as_array().map(|children| {
            children.iter().map(summarize_feature_plan).collect::<Vec<_>>()
        }).unwrap_or_default(),
    })
}

fn artifact_catalog(store: &ArtifactStore) -> Result<Value> {
    let artifacts = store.list_artifacts()?;
    let mut entries = Vec::new();
    let mut candidates = Vec::new();
    for artifact in artifacts.into_iter().take(ARTIFACT_CATALOG_LIMIT) {
        let revisions = store.list_revisions(&artifact.id)?;
        let latest = revisions.last().cloned();
        for candidate in store.list_candidates(&artifact.id)? {
            candidates.push(json!({
                "id": candidate.id,
                "artifactId": candidate.artifact_id,
                "revisionId": candidate.revision_id,
                "templateKey": candidate.template_key,
                "status": candidate.status,
                "createdAt": candidate.created_at,
                "updatedAt": candidate.updated_at,
            }));
        }
        entries.push(json!({
            "id": artifact.id,
            "key": artifact.key,
            "title": artifact.title,
            "createdAt": artifact.created_at,
            "revisionCount": revisions.len(),
            "latestRevision": latest.map(|revision| json!({
                "id": revision.id,
                "number": revision.number,
                "sourceDigest": revision.source_digest,
                "renderedDigest": revision.rendered_digest,
                "createdAt": revision.created_at,
            })),
        }));
    }
    Ok(json!({
        "root": "JCODE_HOME/artifacts",
        "limit": ARTIFACT_CATALOG_LIMIT,
        "artifacts": entries,
        "candidates": candidates.into_iter().take(ARTIFACT_CATALOG_LIMIT).collect::<Vec<_>>(),
    }))
}

fn validate_artifact_bundle(bundle: &ArtifactBundleInput) -> Result<()> {
    if bundle.key.trim().is_empty() || bundle.title.trim().is_empty() {
        return Err(anyhow!("artifact bundle key and title are required"));
    }
    if bundle.source.len() > ARTIFACT_MAX_TEXT_BYTES
        || bundle.rendered.len() > ARTIFACT_MAX_TEXT_BYTES
    {
        return Err(anyhow!(
            "artifact bundle source/rendered exceeds {ARTIFACT_MAX_TEXT_BYTES}-byte limit"
        ));
    }
    Ok(())
}

fn admit_artifact_bundle(store: &ArtifactStore, bundle: ArtifactBundleInput) -> Result<Value> {
    validate_artifact_bundle(&bundle)?;
    let template_key = bundle.template_key.clone();
    let receipt = store.admit_bundle(AdmitBundleInput {
        artifact_key: bundle.key,
        artifact_title: bundle.title,
        source_bytes: bundle.source.into_bytes(),
        rendered_bytes: bundle.rendered.into_bytes(),
        annotation: bundle.annotation.filter(|body| !body.trim().is_empty()),
        candidate_template_key: template_key.clone(),
    })?;
    Ok(json!({
        "version": 1,
        "artifact": receipt.artifact,
        "revision": receipt.revision,
        "candidate": receipt.candidate,
        "annotation": receipt.annotation,
        "templateKey": template_key,
    }))
}

fn reconcile_artifact_effects(
    store: &ArtifactStore,
    effects: &[ExecutionEffect],
    requested_mode: ArtifactMode,
) -> Result<Value> {
    if effects.is_empty() {
        return Ok(json!({
            "mode": requested_mode,
            "status": "read_only",
            "effect_count": 0,
        }));
    }
    if requested_mode == ArtifactMode::Off {
        return Err(anyhow!("Artifact effects require artifact_mode apply"));
    }
    if effects.len() != 1 {
        return Err(anyhow!(
            "Artifact codemode requires exactly one admission effect; received {} effects",
            effects.len()
        ));
    }
    let effect = &effects[0];
    if effect.capability != "artifacts" || effect.operation != "admit_bundle" {
        return Err(anyhow!(
            "unsupported MetaTool capability effect {}.{}",
            effect.capability,
            effect.operation
        ));
    }
    let bundle: ArtifactBundleInput = serde_json::from_value(effect.input.clone())?;
    let receipt = admit_artifact_bundle(store, bundle)?;
    Ok(json!({
        "mode": requested_mode,
        "status": "applied",
        "receipt": receipt,
    }))
}

fn output(title: impl Into<String>, metadata: Value) -> Result<ToolOutput> {
    Ok(ToolOutput::new(serde_json::to_string_pretty(&metadata)?)
        .with_title(title)
        .with_metadata(metadata))
}

#[async_trait]
impl Tool for MetaTool {
    fn name(&self) -> &str {
        "mt"
    }

    fn description(&self) -> &str {
        "Run codemode JavaScript through Jcode's native MetaTool: your code executes inside a sandboxed AgentOS runtime with a live `mt` object offering the full metatool engine and optional governed `mt.tasker` and `mt.artifacts` capabilities. Tasker plan/apply effects are reconciled atomically by the host against the canonical Pi-compatible database; artifact admissions are reconciled into the host artifact store. The guest never receives direct host authority."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["status", "evaluate", "guide"],
                    "description": "status inspects runtime availability; evaluate runs code; guide returns the mt.* API reference grouped by section."
                },
                "code": {"type": "string", "description": "JavaScript body evaluated as an async function with the live `mt` engine object in scope. Use `return` for the final value, e.g. `await mt.put('notes', 'k', { _meta: { summary: 's' }, v: 1 }); return await mt.get('notes', 'k')`. Required for evaluate."},
                "inputs": {
                    "type": "object",
                    "description": "Optional JSON object exposed to the guest as `inputs`."
                },
                "profile": {
                    "type": "string",
                    "enum": ["pure", "workspace-read", "workspace-mutate"],
                    "description": "Execution profile. Defaults to pure; only pure is currently executable."
                },
                "tasker_mode": {
                    "type": "string",
                    "enum": ["off", "plan", "apply"],
                    "description": "Optional native Tasker capability. plan canonically simulates one reconciliation and issues a receipt; apply requires that receipt and executes the exact reviewed change set against the canonical Pi-compatible database."
                },
                "tasker_receipt": {
                    "type": "string",
                    "description": "Receipt id returned by a successful tasker_mode=plan call. Required for apply and bound to the project, canonical snapshot, exact change set, and expiry."
                },
                "tasker_project_id": {
                    "type": "string",
                    "description": "Optional native concurrency project id for bounded candidate, round, and promotion projections. Defaults to the Pi list id."
                },
                "artifact_mode": {
                    "type": "string",
                    "enum": ["off", "apply"],
                    "description": "Optional native artifact-library capability. apply exposes a bounded read-only catalog and admits exactly one bundle through the host artifact store; the guest receives no filesystem or database authority."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: MetaToolInput = serde_json::from_value(input)?;
        match params.action {
            MetaToolAction::Status => output("MetaTool runtime status", Self::status()?),
            MetaToolAction::Guide => {
                let guide: Value = serde_json::from_str(GUIDE_SOURCE)
                    .context("parse embedded MetaTool guide manifest")?;
                output("MetaTool mt.* API guide", guide)
            }
            MetaToolAction::Evaluate => {
                if params.profile != ExecutionProfile::Pure {
                    return Err(anyhow!(MetaToolError::RuntimeUnavailable {
                        message: format!(
                            "profile {:?} is blocked until the native capability broker is implemented",
                            params.profile
                        ),
                    }));
                }
                let source = params
                    .code
                    .filter(|code| !code.trim().is_empty())
                    .ok_or_else(|| anyhow!("code is required for evaluate"))?;
                if source.len() > MAX_SOURCE_BYTES {
                    return Err(anyhow!("code exceeds the {MAX_SOURCE_BYTES}-byte limit"));
                }
                let input_bytes = serde_json::to_vec(&params.inputs)?.len();
                if input_bytes > MAX_INPUT_BYTES {
                    return Err(anyhow!("inputs exceed the {MAX_INPUT_BYTES}-byte limit"));
                }
                let store_root = self.store_root(&ctx)?;
                let tasker_context = if params.tasker_mode == TaskerMode::Off {
                    None
                } else {
                    let store = self.tasker_store(&ctx)?;
                    let project_id = params
                        .tasker_project_id
                        .clone()
                        .unwrap_or_else(|| store.partition().list_id.clone());
                    let (snapshot, snapshot_hash) =
                        tasker_snapshot_for_project(&store, &project_id)?;
                    Some((snapshot, snapshot_hash, project_id))
                };
                let artifact_context = if params.artifact_mode == ArtifactMode::Off {
                    None
                } else {
                    let store = self.artifact_store()?;
                    Some(artifact_catalog(&store)?)
                };
                let capabilities = if tasker_context.is_some() || artifact_context.is_some() {
                    let mut capabilities = serde_json::Map::new();
                    if let Some((snapshot, snapshot_hash, project_id)) = tasker_context.as_ref() {
                        capabilities.insert(
                            "tasker".into(),
                            json!({
                                "mode": params.tasker_mode,
                                "snapshot": snapshot,
                                "snapshot_hash": snapshot_hash,
                                "project_id": project_id,
                            }),
                        );
                    }
                    if let Some(catalog) = artifact_context.as_ref() {
                        capabilities.insert(
                            "artifacts".into(),
                            json!({
                                "mode": params.artifact_mode,
                                "catalog": catalog,
                            }),
                        );
                    }
                    Some(Value::Object(capabilities))
                } else {
                    None
                };
                let request = ExecutionRequest {
                    id: ExecutionId::new(),
                    source,
                    inputs: params.inputs,
                    profile: ExecutionProfile::Pure,
                    limits: ExecutionLimits::default(),
                    store_root: Some(store_root.to_string_lossy().into_owned()),
                    capabilities,
                };
                let result = Self::executor()?.execute(request).await?;
                let tasker_effects: Vec<_> = result
                    .effects
                    .iter()
                    .filter(|effect| effect.capability == "tasker")
                    .cloned()
                    .collect();
                let artifact_effects: Vec<_> = result
                    .effects
                    .iter()
                    .filter(|effect| effect.capability == "artifacts")
                    .cloned()
                    .collect();
                let unsupported_effects: Vec<_> = result
                    .effects
                    .iter()
                    .filter(|effect| {
                        effect.capability != "tasker" && effect.capability != "artifacts"
                    })
                    .cloned()
                    .collect();
                if !unsupported_effects.is_empty() {
                    return Err(anyhow!(
                        "MetaTool guest emitted unsupported capability effects"
                    ));
                }
                let reconciliation = match tasker_context {
                    Some((_, snapshot_hash, _)) => Some({
                        let database_path = self.tasker_database_path();
                        let receipt_root = self.tasker_receipt_root()?;
                        let project_root = ctx.working_dir.clone().ok_or_else(|| {
                            anyhow!("Tasker codemode requires a working directory")
                        })?;
                        let effects = tasker_effects.clone();
                        let mode = params.tasker_mode;
                        let requested_receipt = params.tasker_receipt.clone();
                        let is_candidate_apply = mode == TaskerMode::Apply
                            && effects.len() == 1
                            && effects[0].input["kind"] == CANDIDATE_LANE_COMPATIBILITY_KIND;
                        if is_candidate_apply {
                            let (proposal, receipt, canonical_snapshot_hash) =
                                tokio::task::spawn_blocking({
                                    let database_path = database_path.clone();
                                    let receipt_root = receipt_root.clone();
                                    let project_root = project_root.clone();
                                    let effects = effects.clone();
                                    let requested_receipt = requested_receipt.clone();
                                    move || {
                                        let canonical = std::fs::canonicalize(&project_root)
                                            .unwrap_or(project_root);
                                        let mut store =
                                            PiTaskerStore::open(ProjectPartition::with_db_path(
                                                database_path,
                                                canonical.to_string_lossy().into_owned(),
                                            ))?;
                                        prepare_candidate_lane_apply(
                                            &mut store,
                                            &effects,
                                            &receipt_root,
                                            requested_receipt.as_deref(),
                                        )
                                    }
                                })
                                .await
                                .context("join candidate Tasker authority validation")??;
                            let host = self.candidate_lane_host.clone().ok_or_else(|| {
                                anyhow::Error::new(
                                    CandidateLaneExecutionBlocked::HostExecutorUnavailable,
                                )
                            })?;
                            let operation_id = format!(
                                "candidate-{}-{}",
                                ctx.session_id,
                                next_candidate_request_id()
                            );
                            let host_request = CandidateLaneHostRequest {
                                id: next_candidate_request_id(),
                                operation_id,
                                session_id: ctx.session_id.clone(),
                                working_dir: project_root.to_string_lossy().into_owned(),
                                expected_snapshot_hash: canonical_snapshot_hash.clone(),
                                receipt_id: receipt.id.clone(),
                                proposal: serde_json::to_value(proposal)?,
                            };
                            let cancellation = CancellationToken::new();
                            let cancellation_watcher =
                                ctx.graceful_shutdown_signal.clone().map(|signal| {
                                    let cancellation = cancellation.clone();
                                    tokio::spawn(async move {
                                        if signal.is_set() {
                                            cancellation.cancel();
                                        } else {
                                            signal.notified().await;
                                            cancellation.cancel();
                                        }
                                    })
                                });
                            let response = host
                                .execute_candidate_lanes(host_request, cancellation)
                                .await;
                            if let Some(watcher) = cancellation_watcher {
                                watcher.abort();
                            }
                            let response = response?;
                            let report = validate_candidate_host_response(&response)?;
                            json!({
                                "mode": mode,
                                "status": "applied",
                                "kind": CANDIDATE_LANE_COMPATIBILITY_KIND,
                                "snapshot_hash": canonical_snapshot_hash,
                                "receipt": {
                                    "id": receipt.id,
                                    "change_digest": receipt.change_digest,
                                },
                                "result": report,
                                "redacted": true,
                            })
                        } else {
                            tokio::task::spawn_blocking(move || {
                                let canonical =
                                    std::fs::canonicalize(&project_root).unwrap_or(project_root);
                                let mut store =
                                    PiTaskerStore::open(ProjectPartition::with_db_path(
                                        database_path,
                                        canonical.to_string_lossy().into_owned(),
                                    ))?;
                                reconcile_tasker_effects(
                                    &mut store,
                                    &effects,
                                    mode,
                                    &snapshot_hash,
                                    &receipt_root,
                                    requested_receipt.as_deref(),
                                )
                            })
                            .await
                            .context("join Tasker codemode reconciler")??
                        }
                    }),
                    None if tasker_effects.is_empty() => None,
                    None => {
                        return Err(anyhow!(
                            "MetaTool guest emitted Tasker effects while tasker_mode was off"
                        ));
                    }
                };
                let artifact_admission = match artifact_context {
                    Some(_) => {
                        let store = self.artifact_store()?;
                        Some(reconcile_artifact_effects(
                            &store,
                            &artifact_effects,
                            params.artifact_mode,
                        )?)
                    }
                    None if artifact_effects.is_empty() => None,
                    None => {
                        return Err(anyhow!(
                            "MetaTool guest emitted artifact effects while artifact_mode was off"
                        ));
                    }
                };
                let mut metadata = serde_json::to_value(&result)?;
                if let Some(reconciliation) = reconciliation {
                    metadata["reconciliation"] = reconciliation;
                }
                if let Some(admission) = artifact_admission {
                    metadata["artifactAdmission"] = admission;
                }
                output(
                    format!("MetaTool {:?} in {} ms", result.outcome, result.duration_ms),
                    metadata,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jcode_tool_core::ToolExecutionMode;

    fn context(root: &Path) -> ToolContext {
        ToolContext {
            session_id: "session-metatool-test".into(),
            message_id: "message-metatool-test".into(),
            tool_call_id: "tool-metatool-test".into(),
            working_dir: Some(root.to_path_buf()),
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: ToolExecutionMode::Direct,
        }
    }

    #[test]
    fn schema_exposes_status_evaluate_and_profiles() {
        let definition = MetaTool::new().to_definition();
        assert_eq!(definition.name, "mt");
        assert!(definition.description.contains("codemode"));
        assert_eq!(
            definition.input_schema["properties"]["action"]["enum"],
            json!(["status", "evaluate", "guide"])
        );
        assert_eq!(
            definition.input_schema["properties"]["tasker_mode"]["enum"],
            json!(["off", "plan", "apply"])
        );
        assert_eq!(
            definition.input_schema["properties"]["tasker_project_id"]["type"],
            "string"
        );
        assert_eq!(
            definition.input_schema["properties"]["artifact_mode"]["enum"],
            json!(["off", "apply"])
        );
    }

    #[test]
    fn schema_conforms_to_provider_function_calling_conventions() {
        let definition = MetaTool::new().to_definition();
        let properties = definition.input_schema["properties"]
            .as_object()
            .expect("object properties");
        for (name, schema) in properties {
            assert!(
                schema["type"].is_string(),
                "property {name} must declare a JSON type for provider strict modes"
            );
            assert!(
                schema.get("default").is_none(),
                "property {name} must not carry a default keyword; providers reject it"
            );
        }
        assert_eq!(
            definition.input_schema["required"],
            json!(["action", "intent"])
        );
    }

    #[test]
    fn guide_manifest_exposes_exact_canonical_tasker_method_set() {
        let guide: Value = serde_json::from_str(GUIDE_SOURCE).expect("embedded guide JSON");
        let sections = guide["sections"].as_object().expect("guide sections");
        assert!(guide["notes"].is_array());
        let tasker_methods = sections["tasker-capability"]
            .as_array()
            .expect("public tasker capability methods")
            .iter()
            .map(|method| method["name"].as_str().expect("tasker method name"))
            .collect::<Vec<_>>();
        assert_eq!(
            tasker_methods,
            vec![
                "tasker.status",
                "tasker.snapshot",
                "tasker.list",
                "tasker.show",
                "tasker.search",
                "tasker.ready",
                "tasker.features",
                "tasker.feature",
                "tasker.dependencies",
                "tasker.featureDependencies",
                "tasker.notes",
                "tasker.taskGraph",
                "tasker.taskStructure",
                "tasker.topologySummary",
                "tasker.topologyAnomalies",
                "tasker.topologyPaths",
                "tasker.topologyFrontier",
                "tasker.featureChildren",
                "tasker.taskNeighbors",
                "tasker.featureTree",
                "tasker.resolveTask",
                "tasker.resolveFeature",
                "tasker.featureGates",
                "tasker.featureGate",
                "tasker.lifecycle",
                "tasker.policy",
                "tasker.concurrency",
                "tasker.candidateSets",
                "tasker.candidates",
                "tasker.rounds",
                "tasker.promotions",
                "tasker.candidateSet",
                "tasker.candidate",
                "tasker.round",
                "tasker.promotion",
                "tasker.reconcile",
                "tasker.batch",
                "tasker.plan",
                "tasker.featurePlan",
                "tasker.create",
                "tasker.update",
                "tasker.setState",
                "tasker.addDependency",
                "tasker.addNote",
                "tasker.taskIndex",
                "tasker.createFeature",
                "tasker.updateFeature",
                "tasker.resolveFeatureGate",
                "tasker.setDependencies",
                "tasker.setFeatureDependencies",
                "tasker.link",
                "tasker.unlink",
                "tasker.executeWork",
                "tasker.adjudicateCandidateSet",
                "tasker.promoteCandidate",
                "tasker.recoverPromotion",
            ]
        );
        assert!(sections.get("tasker-compatibility").is_none());
        assert!(
            sections["tasker-capability"]
                .as_array()
                .unwrap()
                .iter()
                .all(|method| method["classification"].is_null())
        );
        assert!(sections["artifacts-capability"].is_array());
        for method in [
            "reconcileConcurrency",
            "createCandidateSet",
            "registerCandidate",
            "submitCandidate",
            "setCandidateSetState",
            "recordRound",
            "recordBallot",
            "preparePromotion",
            "markPromotionRefUpdated",
            "finalizePromotion",
            "abortPromotion",
            "rollbackPromotion",
            "resumePromotion",
            "executeCandidateLanes",
        ] {
            assert!(!SIDECAR_SOURCE.contains(&format!("{method}:")));
        }
        assert!(SIDECAR_SOURCE.contains("executeWork: async"));
        assert!(SIDECAR_SOURCE.contains("adjudicateCandidateSet: async"));
        assert!(SIDECAR_SOURCE.contains("promoteCandidate: async"));
        assert!(SIDECAR_SOURCE.contains("recoverPromotion: async"));
        assert!(SIDECAR_SOURCE.contains("host_executor_unavailable"));
    }

    #[test]
    fn parity_manifest_covers_the_measured_tasker_surface() {
        let manifest: Value = serde_json::from_str(TASKER_PARITY_MANIFEST_SOURCE)
            .expect("checked-in Tasker parity manifest JSON");
        assert_eq!(manifest["audit"]["pi_tasker_store_methods"], 57);
        assert_eq!(manifest["audit"]["public_tasker_actions"], 37);
        assert_eq!(manifest["audit"]["live_mt_tasker_methods_before_slice"], 67);
        assert_eq!(manifest["audit"]["live_mt_tasker_methods_after_slice"], 56);

        let store_methods = manifest["pi_tasker_store_methods"]
            .as_array()
            .expect("manifest Pi method array");
        let actions = manifest["public_tasker_actions"]
            .as_array()
            .expect("manifest action array");
        let store_names = store_methods
            .iter()
            .filter_map(|entry| entry["name"].as_str())
            .collect::<Vec<_>>();
        let action_names = actions
            .iter()
            .filter_map(|entry| entry["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(store_methods.len(), 57);
        assert_eq!(
            store_names,
            vec![
                "open",
                "fork_in_memory",
                "partition",
                "schema_fingerprint",
                "preflight",
                "snapshot",
                "list_meta",
                "ensure_list_meta",
                "list_tasks",
                "list_features",
                "list_dependencies",
                "list_feature_dependencies",
                "list_task_notes",
                "list_feature_notes",
                "task_graph_projection",
                "task_structure_projection",
                "topology_summary_projection",
                "topology_anomalies_projection",
                "topology_paths_projection",
                "topology_frontier_projection",
                "feature_children_projection",
                "task_neighbors_projection",
                "feature_tree_projection",
                "create_visual_artifact",
                "list_visual_artifacts",
                "resolve_task_id",
                "resolve_feature_id",
                "ready_tasks",
                "search_tasks",
                "evaluate_mutation_policy",
                "claim_task",
                "release_claim",
                "get_working_set",
                "enqueue_next_work_unit",
                "create_task",
                "update_task",
                "set_task_indexes",
                "add_task_indexes",
                "remove_task_indexes",
                "batch_execute",
                "plan_import",
                "feature_plan_import",
                "create_feature",
                "update_feature",
                "feature_gates",
                "feature_gate",
                "pending_executable_gate_indexes",
                "resolve_feature_gate",
                "apply_feature_gate_check",
                "apply_feature_gate_checks",
                "set_dependencies",
                "set_feature_dependencies",
                "link_task",
                "unlink_task",
                "append_task_note",
                "append_feature_note",
                "open_concurrency_store",
            ]
        );
        assert_eq!(
            action_names,
            vec![
                "status",
                "ready",
                "show",
                "create_feature",
                "create",
                "add_dependency",
                "set_state",
                "list",
                "search",
                "update",
                "add_note",
                "task_index",
                "feature_list",
                "feature_update",
                "feature_status",
                "link",
                "unlink",
                "batch",
                "plan",
                "feature_plan",
                "claim",
                "release",
                "working_set",
                "next_work_unit",
                "feature_gate",
                "task_artifact_create",
                "task_artifacts",
                "task_stage_report",
                "task_graph",
                "task_structure",
                "topology_summary",
                "topology_anomalies",
                "topology_paths",
                "topology_frontier",
                "feature_children",
                "task_neighbors",
                "feature_tree",
            ]
        );

        let store_counts = store_methods.iter().fold(
            std::collections::BTreeMap::<&str, usize>::new(),
            |mut counts, entry| {
                if let Some(classification) = entry["classification"].as_str() {
                    *counts.entry(classification).or_default() += 1;
                }
                counts
            },
        );
        assert_eq!(store_counts.get("codemode_exposed"), Some(&40));
        assert_eq!(store_counts.get("public_tool_only"), Some(&8));
        assert_eq!(store_counts.get("internal_only"), Some(&9));

        let action_counts = actions.iter().fold(
            std::collections::BTreeMap::<&str, usize>::new(),
            |mut counts, entry| {
                if let Some(classification) = entry["classification"].as_str() {
                    *counts.entry(classification).or_default() += 1;
                }
                counts
            },
        );
        assert_eq!(action_counts.get("codemode_exposed"), Some(&29));
        assert_eq!(action_counts.get("public_tool_only"), Some(&8));
        assert_eq!(
            manifest["concurrency_surface"]["agent_spawning"],
            "host_owned_bounded_headless_inline"
        );
        assert_eq!(
            manifest["concurrency_surface"]["generic_reconcile"],
            "ordinary_only_rejects_concurrency_lifecycle"
        );
        assert_eq!(
            manifest["concurrency_surface"]["ordinary_reconciliation_kinds"],
            json!(ORDINARY_TASKER_RECONCILIATION_KINDS)
        );
        assert_eq!(
            manifest["concurrency_surface"]["ordinary_receipt_methods"],
            json!([
                "reconcile",
                "batch",
                "plan",
                "featurePlan",
                "create",
                "update",
                "setState",
                "addDependency",
                "addNote",
                "taskIndex",
                "createFeature",
                "updateFeature",
                "resolveFeatureGate",
                "setDependencies",
                "setFeatureDependencies",
                "link",
                "unlink"
            ])
        );
        assert_eq!(
            manifest["concurrency_surface"]["blocked_lifecycle_kinds"],
            json!(CONCURRENCY_LIFECYCLE_KINDS)
        );
        assert_eq!(
            manifest["concurrency_surface"]["typed_high_level_methods"],
            json!([
                {"name": "executeWork", "status": "host_executor_unavailable"},
                {"name": "adjudicateCandidateSet", "status": "host_executor_unavailable"},
                {"name": "promoteCandidate", "status": "host_executor_unavailable"},
                {"name": "recoverPromotion", "status": "host_executor_unavailable"}
            ])
        );
        assert_eq!(
            manifest["concurrency_surface"]["compatibility_methods"],
            json!([{
                "name": "executeCandidateLanes",
                "status": "internal_compatibility_deprecated",
                "public": false
            }])
        );
        assert_eq!(
            manifest["concurrency_surface"]["reconciliation_operations"],
            json!([])
        );
    }

    #[test]
    fn runtime_status_is_helpful_when_unavailable() {
        let status = MetaTool::status().unwrap();
        assert!(status["setup"].as_str().unwrap().contains(AGENTOS_PACKAGE));
        assert_eq!(status["version"], AGENTOS_VERSION);
        assert!(status["security_gate"].is_string());
    }

    #[test]
    fn tasker_reconciler_plans_without_writing_then_applies_atomically() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let database = workspace.path().join("tasks.db");
        super::super::tasker::tests::install_pi_schema(&database);
        let partition = ProjectPartition::with_db_path(
            &database,
            workspace.path().to_string_lossy().into_owned(),
        );
        let mut store = PiTaskerStore::open(partition).expect("open Pi-compatible Tasker store");
        let receipts = workspace.path().join("receipts");
        let (_, snapshot_hash) = tasker_snapshot(&store).expect("initial Tasker snapshot");
        let effect = ExecutionEffect {
            capability: "tasker".into(),
            operation: "reconcile".into(),
            input: json!({
                "kind": "batch",
                "mode": "plan",
                "expected_snapshot_hash": snapshot_hash,
                "payload": {
                    "operations": [{
                        "op": "create",
                        "key": "first",
                        "title": "Created through mt.tasker",
                        "dependsOn": [],
                        "notes": []
                    }]
                }
            }),
        };

        let planned = reconcile_tasker_effects(
            &mut store,
            std::slice::from_ref(&effect),
            TaskerMode::Plan,
            &snapshot_hash,
            &receipts,
            None,
        )
        .expect("plan reconciliation");
        assert_eq!(planned["status"], "simulated");
        assert_eq!(planned["validated"], true);
        assert_eq!(planned["simulation"]["created"], 1);
        assert_eq!(planned["simulation"]["operations"][0]["key"], "first");
        assert_eq!(planned["simulation"]["operations"][0]["state"], "todo");
        let repeated = reconcile_tasker_effects(
            &mut store,
            std::slice::from_ref(&effect),
            TaskerMode::Plan,
            &snapshot_hash,
            &receipts,
            None,
        )
        .expect("repeat plan reconciliation");
        assert_eq!(repeated["simulation"], planned["simulation"]);
        assert_eq!(
            repeated["receipt"]["change_digest"],
            planned["receipt"]["change_digest"]
        );
        assert!(store.list_tasks(None).expect("list after plan").is_empty());

        let receipt = planned["receipt"]["id"]
            .as_str()
            .expect("receipt id")
            .to_owned();
        let expired_receipt = repeated["receipt"]["id"]
            .as_str()
            .expect("repeated receipt id")
            .to_owned();
        let expired_path = receipt_path(&receipts, &expired_receipt).expect("expired receipt path");
        let mut expired_record: TaskerPlanReceipt =
            serde_json::from_slice(&std::fs::read(&expired_path).expect("read repeated receipt"))
                .expect("parse repeated receipt");
        expired_record.expires_at = 0;
        std::fs::write(
            &expired_path,
            serde_json::to_vec_pretty(&expired_record).expect("serialize expired receipt"),
        )
        .expect("expire repeated receipt");

        let mut apply_effect = effect;
        apply_effect.input["mode"] = json!("apply");
        let missing = reconcile_tasker_effects(
            &mut store,
            std::slice::from_ref(&apply_effect),
            TaskerMode::Apply,
            &snapshot_hash,
            &receipts,
            None,
        )
        .expect_err("apply without a receipt must fail");
        assert!(missing.to_string().contains("receipt"));

        let expired = reconcile_tasker_effects(
            &mut store,
            std::slice::from_ref(&apply_effect),
            TaskerMode::Apply,
            &snapshot_hash,
            &receipts,
            Some(&expired_receipt),
        )
        .expect_err("expired receipt must fail");
        assert!(expired.to_string().contains("expired"));
        assert!(
            store
                .list_tasks(None)
                .expect("list after expiry")
                .is_empty()
        );

        let mut mismatched_effect = apply_effect.clone();
        mismatched_effect.input["payload"]["operations"][0]["title"] =
            json!("Changed after review");
        let mismatch = reconcile_tasker_effects(
            &mut store,
            &[mismatched_effect],
            TaskerMode::Apply,
            &snapshot_hash,
            &receipts,
            Some(&receipt),
        )
        .expect_err("receipt must reject a changed apply payload");
        assert!(mismatch.to_string().contains("change-set mismatch"));
        assert!(
            store
                .list_tasks(None)
                .expect("list after mismatch")
                .is_empty()
        );

        let applied = reconcile_tasker_effects(
            &mut store,
            &[apply_effect],
            TaskerMode::Apply,
            &snapshot_hash,
            &receipts,
            Some(&receipt),
        )
        .expect("apply reconciliation");
        assert_eq!(applied["status"], "applied");
        let tasks = store.list_tasks(None).expect("list after apply");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Created through mt.tasker");
    }

    #[test]
    fn candidate_lane_apply_is_blocked_without_server_executor() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let database = workspace.path().join("tasks.db");
        super::super::tasker::tests::install_pi_schema(&database);
        let partition = ProjectPartition::with_db_path(
            &database,
            workspace.path().to_string_lossy().into_owned(),
        );
        let mut store = PiTaskerStore::open(partition).expect("open Pi-compatible Tasker store");
        let receipts = workspace.path().join("receipts");
        let (_, snapshot_hash) = tasker_snapshot(&store).expect("initial Tasker snapshot");
        let effect = ExecutionEffect {
            capability: "tasker".into(),
            operation: "reconcile".into(),
            input: json!({
                "kind": CANDIDATE_LANE_COMPATIBILITY_KIND,
                "mode": "plan",
                "expected_snapshot_hash": snapshot_hash,
                "payload": {
                    "policy": {"kind": "speculative", "max_candidates": 2},
                    "laneCount": 2,
                    "acceptance": {
                        "validationCommands": [{"program": "true", "args": []}],
                        "acceptanceCriteria": "candidate must pass",
                        "resourceIntents": []
                    },
                    "prompt": "Implement the bounded candidate task",
                    "limits": {"maxLanes": 2, "laneTimeoutMs": 1000}
                }
            }),
        };

        let planned = reconcile_tasker_effects(
            &mut store,
            std::slice::from_ref(&effect),
            TaskerMode::Plan,
            &snapshot_hash,
            &receipts,
            None,
        )
        .expect("candidate-lane plan");
        assert_eq!(planned["status"], "simulated");
        assert_eq!(planned["simulation"]["status"], "blocked");
        assert_eq!(
            planned["simulation"]["blockedReason"],
            "host_executor_unavailable"
        );
        assert!(planned["receipt"]["id"].is_string());
        assert!(store.list_tasks(None).expect("list after plan").is_empty());

        let receipt = planned["receipt"]["id"].as_str().expect("receipt id");
        let mut apply_effect = effect;
        apply_effect.input["mode"] = json!("apply");
        let blocked = reconcile_tasker_effects(
            &mut store,
            &[apply_effect],
            TaskerMode::Apply,
            &snapshot_hash,
            &receipts,
            Some(receipt),
        )
        .expect_err("candidate apply must remain blocked");
        assert!(blocked.to_string().contains("host_executor_unavailable"));
        assert!(
            store
                .list_tasks(None)
                .expect("list after blocked apply")
                .is_empty()
        );
    }

    #[test]
    fn candidate_host_response_requires_redacted_structured_submission() {
        let submitted_report = json!({
            "candidateSetId": format!("cset_{}", uuid::Uuid::new_v4()),
            "revision": 7,
            "outcomes": [{
                "candidateId": format!("cand_{}", uuid::Uuid::new_v4()),
                "state": "submitted",
                "submission": {
                    "resultCommit": "host-commit",
                    "diffDigest": null,
                    "summary": null,
                    "evidence": []
                },
                "failure": null
            }]
        });
        let response = CandidateLaneHostResponse {
            status: "completed".into(),
            redacted: true,
            report: submitted_report,
        };
        assert_eq!(
            validate_candidate_host_response(&response)
                .expect("structured submitted report")
                .submitted_count(),
            1
        );

        let metadata_only = json!({
            "candidateSetId": format!("cset_{}", uuid::Uuid::new_v4()),
            "revision": 8,
            "outcomes": [{
                "candidateId": format!("cand_{}", uuid::Uuid::new_v4()),
                "state": "abandoned",
                "submission": null,
                "failure": null
            }]
        });
        let metadata_response = CandidateLaneHostResponse {
            status: "completed".into(),
            redacted: true,
            report: metadata_only,
        };
        let error = validate_candidate_host_response(&metadata_response)
            .expect_err("metadata-only success must be rejected");
        assert!(error.to_string().contains("metadata-only success"));

        let unredacted = CandidateLaneHostResponse {
            redacted: false,
            ..response
        };
        let error = validate_candidate_host_response(&unredacted)
            .expect_err("unredacted host result must be rejected");
        assert!(error.to_string().contains("redacted completion"));
    }

    #[test]
    fn tasker_reconciler_rejects_all_concurrency_lifecycle_kinds() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let database = workspace.path().join("tasks.db");
        super::super::tasker::tests::install_pi_schema(&database);
        let partition = ProjectPartition::with_db_path(
            &database,
            workspace.path().to_string_lossy().into_owned(),
        );
        let mut store = PiTaskerStore::open(partition).expect("open Pi-compatible Tasker store");
        let receipts = workspace.path().join("receipts");

        for &kind in CONCURRENCY_LIFECYCLE_KINDS {
            let (_, snapshot_hash) = tasker_snapshot(&store).expect("Tasker snapshot");
            let effect = ExecutionEffect {
                capability: "tasker".into(),
                operation: "reconcile".into(),
                input: json!({
                    "kind": kind,
                    "mode": "plan",
                    "expected_snapshot_hash": snapshot_hash,
                    "payload": {}
                }),
            };
            let error = reconcile_tasker_effects(
                &mut store,
                std::slice::from_ref(&effect),
                TaskerMode::Plan,
                &snapshot_hash,
                &receipts,
                None,
            )
            .expect_err("generic ordinary reconcile must reject concurrency lifecycle writes");
            assert_eq!(
                error.to_string(),
                format!(
                    "Tasker generic ordinary reconcile rejects concurrency lifecycle kind: {kind}"
                )
            );
        }
        assert!(
            store
                .list_tasks(None)
                .expect("list after rejection")
                .is_empty()
        );
    }

    #[test]
    fn tasker_reconciler_rejects_stale_snapshot_identity() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let database = workspace.path().join("tasks.db");
        super::super::tasker::tests::install_pi_schema(&database);
        let partition = ProjectPartition::with_db_path(
            &database,
            workspace.path().to_string_lossy().into_owned(),
        );
        let mut store = PiTaskerStore::open(partition).expect("open Pi-compatible Tasker store");
        let effect = ExecutionEffect {
            capability: "tasker".into(),
            operation: "reconcile".into(),
            input: json!({
                "kind": "batch",
                "mode": "apply",
                "expected_snapshot_hash": "stale",
                "payload": {"operations": []}
            }),
        };

        let error = reconcile_tasker_effects(
            &mut store,
            &[effect],
            TaskerMode::Apply,
            "current",
            workspace.path(),
            None,
        )
        .expect_err("stale snapshot must fail");
        assert!(error.to_string().contains("snapshot identity mismatch"));
    }

    #[test]
    fn tasker_plan_simulation_rejects_invalid_operations_without_writing() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let database = workspace.path().join("tasks.db");
        super::super::tasker::tests::install_pi_schema(&database);
        let partition = ProjectPartition::with_db_path(
            &database,
            workspace.path().to_string_lossy().into_owned(),
        );
        let mut store = PiTaskerStore::open(partition).expect("open Pi-compatible Tasker store");
        let (_, snapshot_hash) = tasker_snapshot(&store).expect("initial Tasker snapshot");
        let effect = ExecutionEffect {
            capability: "tasker".into(),
            operation: "reconcile".into(),
            input: json!({
                "kind": "batch",
                "mode": "plan",
                "expected_snapshot_hash": snapshot_hash,
                "payload": {
                    "operations": [{
                        "op": "create",
                        "key": "invalid",
                        "title": "Invalid simulated task",
                        "dependsOn": ["missing"],
                        "notes": []
                    }]
                }
            }),
        };

        let error = reconcile_tasker_effects(
            &mut store,
            &[effect],
            TaskerMode::Plan,
            &snapshot_hash,
            workspace.path(),
            None,
        )
        .expect_err("canonical validation should reject missing dependency");
        assert!(error.to_string().contains("invalid reference"));
        assert!(store.list_tasks(None).expect("source list").is_empty());
    }

    #[tokio::test]
    #[ignore = "requires a pinned AgentOS runtime via JCODE_METATOOL_RUNTIME_DIR"]
    async fn evaluates_codemode_with_durable_store_across_calls() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let store_root = tempfile::tempdir().expect("store tempdir");

        let write_tool = MetaTool::with_store_root(store_root.path().to_path_buf());
        let written = write_tool
            .execute(
                json!({
                    "action": "evaluate",
                    "code": "await mt.put('probe', 'first', { _meta: { summary: 'codemode probe' }, n: inputs.n }); const got = await mt.get('probe', 'first'); return { doubled: got.n * 2 };",
                    "inputs": {"n": 21},
                    "profile": "pure"
                }),
                context(workspace.path()),
            )
            .await
            .expect("codemode write evaluation")
            .metadata
            .expect("write metadata");
        assert_eq!(written["outcome"], "succeeded");
        assert_eq!(written["value"]["doubled"], 42);

        let read_tool = MetaTool::with_store_root(store_root.path().to_path_buf());
        let read = read_tool
            .execute(
                json!({
                    "action": "evaluate",
                    "code": "const got = await mt.get('probe', 'first'); const hits = await mt.search('codemode'); return { persisted: got?.n ?? null, hits: hits.filter((hit) => hit.collection === 'probe').length };",
                    "profile": "pure"
                }),
                context(workspace.path()),
            )
            .await
            .expect("codemode read evaluation")
            .metadata
            .expect("read metadata");
        assert_eq!(read["outcome"], "succeeded");
        assert_eq!(read["value"]["persisted"], 21);
        assert_eq!(read["value"]["hits"], 1);
    }

    #[tokio::test]
    #[ignore = "requires a pinned AgentOS runtime via JCODE_METATOOL_RUNTIME_DIR"]
    async fn evaluates_tasker_plan_then_apply_through_agentos() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let store_root = tempfile::tempdir().expect("store tempdir");
        let tasker_database = workspace.path().join("tasks.db");
        super::super::tasker::tests::install_pi_schema(&tasker_database);
        let tool = MetaTool::with_store_and_tasker_roots(
            store_root.path().to_path_buf(),
            tasker_database.clone(),
        );
        let code = "await mt.tasker.batch([{ op: 'create', key: 'first', title: 'AgentOS reconciled task', dependsOn: [], notes: [] }]); return mt.tasker.status();";

        let planned = tool
            .execute(
                json!({
                    "action": "evaluate",
                    "code": code,
                    "profile": "pure",
                    "tasker_mode": "plan"
                }),
                context(workspace.path()),
            )
            .await
            .expect("Tasker plan evaluation")
            .metadata
            .expect("plan metadata");
        assert_eq!(planned["outcome"], "succeeded");
        assert_eq!(planned["reconciliation"]["status"], "simulated");
        assert_eq!(planned["reconciliation"]["validated"], true);
        assert_eq!(planned["reconciliation"]["simulation"]["created"], 1);
        let receipt = planned["reconciliation"]["receipt"]["id"]
            .as_str()
            .expect("plan receipt id");

        let partition = ProjectPartition::with_db_path(
            &tasker_database,
            workspace.path().to_string_lossy().into_owned(),
        );
        let store = PiTaskerStore::open(partition.clone()).expect("open Tasker after plan");
        assert!(store.list_tasks(None).expect("list after plan").is_empty());

        let applied = tool
            .execute(
                json!({
                    "action": "evaluate",
                    "code": code,
                    "profile": "pure",
                    "tasker_mode": "apply",
                    "tasker_receipt": receipt
                }),
                context(workspace.path()),
            )
            .await
            .expect("Tasker apply evaluation")
            .metadata
            .expect("apply metadata");
        assert_eq!(applied["outcome"], "succeeded");
        assert_eq!(applied["reconciliation"]["status"], "applied");

        let store = PiTaskerStore::open(partition).expect("open Tasker after apply");
        let tasks = store.list_tasks(None).expect("list after apply");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "AgentOS reconciled task");
    }

    #[test]
    fn artifact_reconciler_admits_one_bundle_to_host_store() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let store = ArtifactStore::open_migrate(
            workspace.path().join("artifacts.sqlite3"),
            workspace.path().join("assets"),
        )
        .expect("open artifact store");
        let effect = ExecutionEffect {
            capability: "artifacts".into(),
            operation: "admit_bundle".into(),
            input: json!({
                "key": "deck/a",
                "title": "Deck A",
                "source": "# Deck",
                "rendered": "<h1>Deck</h1>",
                "annotation": "first admission",
                "templateKey": "html-deck"
            }),
        };

        let admitted = reconcile_artifact_effects(&store, &[effect], ArtifactMode::Apply)
            .expect("artifact admission");

        assert_eq!(admitted["status"], "applied");
        assert_eq!(admitted["receipt"]["artifact"]["key"], "deck/a");
        assert_eq!(admitted["receipt"]["templateKey"], "html-deck");
        let artifacts = store.list_artifacts().expect("list artifacts");
        assert_eq!(artifacts.len(), 1);
        let revisions = store
            .list_revisions(&artifacts[0].id)
            .expect("list revisions");
        assert_eq!(revisions.len(), 1);
        assert_eq!(store.read_source_bytes(&revisions[0]).unwrap(), b"# Deck");
        assert_eq!(store.list_annotations(&artifacts[0].id).unwrap().len(), 1);
        let candidates = store.list_candidates(&artifacts[0].id).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].template_key, "html-deck");
    }

    #[test]
    fn artifact_reconciler_rejects_off_multiple_and_wrong_operation() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let store = ArtifactStore::open_migrate(
            workspace.path().join("artifacts.sqlite3"),
            workspace.path().join("assets"),
        )
        .expect("open artifact store");
        let effect = ExecutionEffect {
            capability: "artifacts".into(),
            operation: "admit_bundle".into(),
            input: json!({"key":"k","title":"T","source":"s","rendered":"r"}),
        };
        assert!(
            reconcile_artifact_effects(&store, std::slice::from_ref(&effect), ArtifactMode::Off)
                .unwrap_err()
                .to_string()
                .contains("artifact_mode apply")
        );
        assert!(
            reconcile_artifact_effects(
                &store,
                &[effect.clone(), effect.clone()],
                ArtifactMode::Apply
            )
            .unwrap_err()
            .to_string()
            .contains("exactly one")
        );
        let wrong = ExecutionEffect {
            operation: "nope".into(),
            ..effect
        };
        assert!(
            reconcile_artifact_effects(&store, &[wrong], ArtifactMode::Apply)
                .unwrap_err()
                .to_string()
                .contains("unsupported")
        );
    }
}
