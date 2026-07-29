use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use jcode_metatool_runtime::{
    AGENTOS_PACKAGE, AGENTOS_VERSION, AgentOsExecutor, AgentOsRuntimeConfig, JavaScriptExecutor,
    SIDECAR_SOURCE,
};
use jcode_metatool_types::{
    ExecutionId, ExecutionLimits, ExecutionProfile, ExecutionRequest, MetaToolError,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{Tool, ToolContext, ToolOutput};

const RUNTIME_DIR_ENV: &str = "JCODE_METATOOL_RUNTIME_DIR";
const NODE_BINARY_ENV: &str = "JCODE_METATOOL_NODE";
const SIDECAR_FILE: &str = "jcode-agentos-sidecar.mjs";
const MAX_SOURCE_BYTES: usize = 64 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;

pub struct MetaTool;

impl MetaTool {
    pub fn new() -> Self {
        Self
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

    fn materialize_sidecar(runtime_dir: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(runtime_dir).with_context(|| {
            format!(
                "create MetaTool runtime directory {}",
                runtime_dir.display()
            )
        })?;
        let path = Self::sidecar_path(runtime_dir);
        let expected = AgentOsRuntimeConfig::expected_sidecar_sha256();
        if path.exists() {
            let current_matches = std::fs::read(&path)
                .ok()
                .map(|source| {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(source)) == expected
                })
                .unwrap_or(false);
            if !current_matches {
                return Err(anyhow!(MetaToolError::RuntimeUnavailable {
                    message: format!(
                        "refusing to replace an AgentOS sidecar with an unexpected digest at {}",
                        path.display()
                    ),
                }));
            }
        } else {
            use std::io::Write;

            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .context("create MetaTool sidecar")?;
            file.write_all(SIDECAR_SOURCE.as_bytes())
                .context("write MetaTool sidecar")?;
            file.sync_all().context("sync MetaTool sidecar")?;
            let written_matches = std::fs::read(&path)
                .ok()
                .map(|source| {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(source)) == expected
                })
                .unwrap_or(false);
            if !written_matches {
                return Err(anyhow!(MetaToolError::RuntimeUnavailable {
                    message: "written AgentOS sidecar failed its integrity check".to_owned(),
                }));
            }
        }
        Ok(path)
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
        "Execute bounded JavaScript through Jcode's experimental native MetaTool runtime. Use action=status to inspect availability. Only the pure profile is executable until the capability broker exists."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {"type": "string", "enum": ["status", "evaluate"], "default": "evaluate"},
                "code": {"type": "string", "description": "Async JavaScript source. Required for evaluate."},
                "inputs": {"description": "Clone-safe JSON value exposed to the guest as inputs."},
                "profile": {"type": "string", "enum": ["pure", "workspace-read", "workspace-mutate"], "default": "pure"}
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let params: MetaToolInput = serde_json::from_value(input)?;
        match params.action {
            MetaToolAction::Status => output("MetaTool runtime status", Self::status()?),
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
                let request = ExecutionRequest {
                    id: ExecutionId::new(),
                    source,
                    inputs: params.inputs,
                    profile: ExecutionProfile::Pure,
                    limits: ExecutionLimits::default(),
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
        assert!(definition.description.contains("pure profile"));
        assert_eq!(
            definition.input_schema["properties"]["action"]["enum"],
            json!(["status", "evaluate"])
        );
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
    async fn evaluates_pure_javascript_through_public_tool_contract() {
        let workspace = tempfile::tempdir().expect("workspace tempdir");
        let result = MetaTool::new()
            .execute(
                json!({
                    "action": "evaluate",
                    "code": "({ answer: inputs.left * inputs.right })",
                    "inputs": {"left": 6, "right": 7},
                    "profile": "pure"
                }),
                context(workspace.path()),
            )
            .await
            .expect("evaluate pure JavaScript")
            .metadata
            .expect("execution metadata");
        assert_eq!(result["outcome"], "succeeded");
        assert_eq!(result["value"]["answer"], 42);
    }
}
