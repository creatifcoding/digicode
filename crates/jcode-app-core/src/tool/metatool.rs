use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jcode_metatool_runtime::{
    AGENTOS_PACKAGE, AGENTOS_VERSION, AgentOsExecutor, AgentOsRuntimeConfig, GUEST_ENGINE_FILE,
    JavaScriptExecutor, SIDECAR_SOURCE,
};
use jcode_metatool_types::{
    ExecutionId, ExecutionLimits, ExecutionProfile, ExecutionRequest, MetaToolError,
};
use serde::Deserialize;
use serde_json::{Value, json};

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

pub struct MetaTool {
    store_root_override: Option<PathBuf>,
}

impl MetaTool {
    pub fn new() -> Self {
        Self {
            store_root_override: None,
        }
    }

    #[cfg(test)]
    fn with_store_root(store_root: PathBuf) -> Self {
        Self {
            store_root_override: Some(store_root),
        }
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
        "Run codemode JavaScript through Jcode's native MetaTool: your code executes inside a sandboxed AgentOS runtime with a live `mt` object offering the full metatool engine (durable workspace store: mt.put/get/query/search/collections, fluent mt.from/mt.into builders, procedures, catalog). State persists across calls in a workspace-scoped store. Use action=status to inspect availability."
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
                let request = ExecutionRequest {
                    id: ExecutionId::new(),
                    source,
                    inputs: params.inputs,
                    profile: ExecutionProfile::Pure,
                    limits: ExecutionLimits::default(),
                    store_root: Some(store_root.to_string_lossy().into_owned()),
                };
                let result = Self::executor()?.execute(request).await?;
                output(
                    format!("MetaTool {:?} in {} ms", result.outcome, result.duration_ms),
                    serde_json::to_value(result)?,
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
    }

    #[test]
    fn runtime_status_is_helpful_when_unavailable() {
        let status = MetaTool::status().unwrap();
        assert!(status["setup"].as_str().unwrap().contains(AGENTOS_PACKAGE));
        assert_eq!(status["version"], AGENTOS_VERSION);
        assert!(status["security_gate"].is_string());
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
}
