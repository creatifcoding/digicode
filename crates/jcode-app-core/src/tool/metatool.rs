use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jcode_metatool_runtime::{
    AGENTOS_PACKAGE, AGENTOS_VERSION, AgentOsExecutor, AgentOsRuntimeConfig, GUEST_ENGINE_FILE,
    JavaScriptExecutor, SIDECAR_SOURCE,
};
use jcode_metatool_types::{
    ExecutionEffect, ExecutionId, ExecutionLimits, ExecutionProfile, ExecutionRequest,
    MetaToolError,
};
use jcode_tasker_pi::{
    BatchOperation, FeaturePlanFeature, FeaturePlanInput, PiTaskerStore, PlanTask, ProjectPartition,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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
const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const TASKER_SNAPSHOT_LIMIT: usize = 500;
const TASKER_RECEIPT_TTL_SECONDS: u64 = 30 * 60;
const TASKER_RECEIPT_PREFIX: &str = "tpr_";

pub struct MetaTool {
    store_root_override: Option<PathBuf>,
    tasker_database_path_override: Option<PathBuf>,
    tasker_receipt_root_override: Option<PathBuf>,
}

impl MetaTool {
    pub fn new() -> Self {
        Self {
            store_root_override: None,
            tasker_database_path_override: None,
            tasker_receipt_root_override: None,
        }
    }

    #[cfg(test)]
    fn with_store_root(store_root: PathBuf) -> Self {
        Self {
            store_root_override: Some(store_root),
            tasker_database_path_override: None,
            tasker_receipt_root_override: None,
        }
    }

    #[cfg(test)]
    fn with_store_and_tasker_roots(store_root: PathBuf, tasker_database_path: PathBuf) -> Self {
        let tasker_receipt_root = tasker_database_path.with_extension("receipts");
        Self {
            store_root_override: Some(store_root),
            tasker_database_path_override: Some(tasker_database_path),
            tasker_receipt_root_override: Some(tasker_receipt_root),
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
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TaskerMode {
    #[default]
    Off,
    Plan,
    Apply,
}

#[derive(Debug, Deserialize)]
struct TaskerEffectInput {
    kind: String,
    payload: Value,
    mode: TaskerMode,
    expected_snapshot_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TaskerPlanReceipt {
    version: u8,
    id: String,
    project_root: String,
    list_id: String,
    snapshot_hash: String,
    change_digest: String,
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
    if receipt.change_digest != tasker_change_digest(input)? {
        return Err(anyhow!("Tasker plan receipt change-set mismatch"));
    }
    Ok(receipt)
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

fn tasker_snapshot(store: &PiTaskerStore) -> Result<(Value, String)> {
    let snapshot = store.snapshot()?;
    let ready = store.ready_tasks()?;
    let canonical = json!({
        "list_meta": snapshot.list_meta,
        "tasks": snapshot.tasks,
        "dependencies": snapshot.dependencies,
        "features": snapshot.features,
        "feature_dependencies": snapshot.feature_dependencies,
        "task_notes": snapshot.task_notes,
        "feature_notes": snapshot.feature_notes,
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
    let partition = store.partition();
    let bounded = json!({
        "partition": {
            "database_path": partition.db_path,
            "project_root": partition.project_root,
            "list_id": partition.list_id,
        },
        "schema_fingerprint": store.schema_fingerprint()?,
        "counts": {
            "tasks": tasks.len(),
            "features": features.len(),
            "dependencies": canonical["dependencies"].as_array().map_or(0, Vec::len),
            "ready": ready.len(),
        },
        "tasks": tasks.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "features": features.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "ready": ready.into_iter().take(TASKER_SNAPSHOT_LIMIT).collect::<Vec<_>>(),
        "truncated": canonical["tasks"].as_array().is_some_and(|items| items.len() > TASKER_SNAPSHOT_LIMIT)
            || canonical["features"].as_array().is_some_and(|items| items.len() > TASKER_SNAPSHOT_LIMIT),
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
    let input: TaskerEffectInput = serde_json::from_value(effect.input.clone())?;
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

    if requested_mode == TaskerMode::Plan {
        if requested_receipt.is_some() {
            return Err(anyhow!("tasker_receipt is valid only for apply mode"));
        }
        let mut simulation = store.fork_in_memory()?;
        let simulated = execute_tasker_reconciliation(&mut simulation, &input)?;
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

    let (_, current_hash) = tasker_snapshot(store)?;
    if current_hash != initial_snapshot_hash {
        return Err(anyhow!(
            "Tasker state changed during codemode evaluation; rerun to reconcile against the current snapshot"
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

fn execute_tasker_reconciliation(
    store: &mut PiTaskerStore,
    input: &TaskerEffectInput,
) -> Result<Value> {
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
        other => Err(anyhow!("unsupported Tasker reconciliation kind: {other}")),
    }
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
        _ => Value::Null,
    }
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
        "Run codemode JavaScript through Jcode's native MetaTool: your code executes inside a sandboxed AgentOS runtime with a live `mt` object offering the full metatool engine and an optional governed `mt.tasker` capability. Tasker plan/apply effects are reconciled atomically by the host against the canonical Pi-compatible database; the guest never receives direct host authority."
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
                    let (snapshot, snapshot_hash) = tasker_snapshot(&store)?;
                    Some((snapshot, snapshot_hash))
                };
                let capabilities = tasker_context.as_ref().map(|(snapshot, snapshot_hash)| {
                    json!({
                        "tasker": {
                            "mode": params.tasker_mode,
                            "snapshot": snapshot,
                            "snapshot_hash": snapshot_hash,
                        }
                    })
                });
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
                let reconciliation = match tasker_context {
                    Some((_, snapshot_hash)) => {
                        let database_path = self.tasker_database_path();
                        let receipt_root = self.tasker_receipt_root()?;
                        let project_root = ctx.working_dir.clone().ok_or_else(|| {
                            anyhow!("Tasker codemode requires a working directory")
                        })?;
                        let effects = result.effects.clone();
                        let mode = params.tasker_mode;
                        let requested_receipt = params.tasker_receipt.clone();
                        Some(
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
                            .context("join Tasker codemode reconciler")??,
                        )
                    }
                    None if result.effects.is_empty() => None,
                    None => {
                        return Err(anyhow!(
                            "MetaTool guest emitted capability effects while tasker_mode was off"
                        ));
                    }
                };
                let mut metadata = serde_json::to_value(&result)?;
                if let Some(reconciliation) = reconciliation {
                    metadata["reconciliation"] = reconciliation;
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
    fn guide_manifest_parses_and_covers_the_measured_surface() {
        let guide: Value = serde_json::from_str(GUIDE_SOURCE).expect("embedded guide JSON");
        let sections = guide["sections"].as_object().expect("guide sections");
        let method_count: usize = sections
            .values()
            .filter_map(|section| section.as_array())
            .map(|methods| methods.len())
            .sum();
        assert!(
            method_count >= 39,
            "guide should document the measured live surface, found {method_count}"
        );
        assert!(guide["notes"].is_array());
        assert!(sections["tasker-capability"].is_array());
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
}
