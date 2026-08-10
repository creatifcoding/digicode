// Bounded, deterministic runtime-retention smokes for the capabilities that
// must survive an exact-commit build and registry rebuild. These tests stay
// inside the real app-core Registry and use only temporary state. They do not
// start a server, invoke a live provider, or touch the user's desktop.

use std::sync::Arc;

const RUNTIME_SMOKE_INTENT: &str = "bounded runtime retention smoke";
const RETAINED_RUNTIME_TOOLS: &[&str] = &["mt", "tasker", "session_search", "swarm", "side_panel"];

#[cfg(target_os = "linux")]
const LINUX_RUNTIME_TOOL: &str = "linux_computer_use";

fn runtime_tool_context(root: &std::path::Path, session_id: &str) -> ToolContext {
    ToolContext {
        session_id: session_id.to_string(),
        message_id: "runtime-retention-message".to_string(),
        tool_call_id: "runtime-retention-call".to_string(),
        working_dir: Some(root.to_path_buf()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    }
}

fn retained_definition_snapshot(
    definitions: &[crate::message::ToolDefinition],
) -> std::collections::BTreeMap<String, serde_json::Value> {
    definitions
        .iter()
        .filter(|definition| {
            RETAINED_RUNTIME_TOOLS
                .iter()
                .any(|name| *name == definition.name)
                || {
                    #[cfg(target_os = "linux")]
                    {
                        definition.name == LINUX_RUNTIME_TOOL
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        false
                    }
                }
        })
        .map(|definition| {
            (
                definition.name.clone(),
                serde_json::json!({
                    "description": definition.description,
                    "input_schema": definition.input_schema,
                }),
            )
        })
        .collect()
}

fn required_schema_values(schema: &serde_json::Value) -> Vec<&str> {
    schema["required"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect()
}

struct RuntimeCatalogProvider;

#[async_trait]
impl Provider for RuntimeCatalogProvider {
    async fn complete(
        &self,
        _messages: &[crate::message::Message],
        _tools: &[crate::message::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        unreachable!("runtime catalog smoke provider must not be invoked")
    }

    fn name(&self) -> &str {
        "runtime-catalog-provider"
    }

    fn model(&self) -> String {
        "runtime-catalog-model".to_string()
    }

    fn available_models_display(&self) -> Vec<String> {
        vec!["runtime-catalog-model".to_string()]
    }

    fn model_routes(&self) -> Vec<jcode_provider_core::ModelRoute> {
        vec![jcode_provider_core::ModelRoute {
            model: "runtime-catalog-model".to_string(),
            provider: "Runtime Catalog".to_string(),
            api_method: "runtime-catalog-api".to_string(),
            available: true,
            detail: "deterministic registry smoke route".to_string(),
            cheapness: None,
        }]
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self)
    }
}

struct RuntimeRetentionEnv {
    _lock: std::sync::MutexGuard<'static, ()>,
    previous_home: Option<std::ffi::OsString>,
    previous_jcode_home: Option<std::ffi::OsString>,
}

impl RuntimeRetentionEnv {
    fn new(home: &std::path::Path) -> Self {
        let lock = crate::storage::lock_test_env();
        let previous_home = std::env::var_os("HOME");
        let previous_jcode_home = std::env::var_os("JCODE_HOME");
        crate::env::set_var("HOME", home);
        crate::env::set_var("JCODE_HOME", home);
        Self {
            _lock: lock,
            previous_home,
            previous_jcode_home,
        }
    }
}

impl Drop for RuntimeRetentionEnv {
    fn drop(&mut self) {
        if let Some(previous) = self.previous_home.take() {
            crate::env::set_var("HOME", previous);
        } else {
            crate::env::remove_var("HOME");
        }
        if let Some(previous) = self.previous_jcode_home.take() {
            crate::env::set_var("JCODE_HOME", previous);
        } else {
            crate::env::remove_var("JCODE_HOME");
        }
    }
}

fn install_runtime_tasker_schema(database_path: &std::path::Path) {
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).expect("create isolated Pi Tasker database directory");
    }
    let connection = rusqlite::Connection::open(database_path)
        .expect("open isolated Pi Tasker database for schema setup");
    connection
        .execute_batch(
            r#"
            CREATE TABLE task_lists (list_id TEXT NOT NULL, project_root TEXT NOT NULL, name TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (list_id, project_root));
            CREATE TABLE tasks (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, display_id INTEGER NOT NULL, title TEXT NOT NULL, description TEXT, state TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, indexes TEXT DEFAULT '[]', feature_id TEXT);
            CREATE TABLE task_dependencies (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, depends_on TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE task_notes (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, category TEXT, content TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE features (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, display_id INTEGER NOT NULL, parent_feature_id TEXT, title TEXT NOT NULL, description TEXT, state TEXT NOT NULL DEFAULT 'open', priority TEXT DEFAULT 'medium', tags TEXT DEFAULT '[]', brief TEXT, acceptance TEXT DEFAULT '[]', owner TEXT, gates TEXT DEFAULT '[]', indexes TEXT DEFAULT '[]', depth INTEGER DEFAULT 0, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
            CREATE TABLE feature_dependencies (id TEXT PRIMARY KEY, feature_id TEXT NOT NULL, depends_on TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE feature_notes (id TEXT PRIMARY KEY, feature_id TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, category TEXT, content TEXT NOT NULL, created_at INTEGER NOT NULL);
            CREATE TABLE tasker_session_instances (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, agent_id TEXT NOT NULL, session_id TEXT NOT NULL, session_file TEXT, pid INTEGER NOT NULL, model TEXT, leaf_id_at_start TEXT, current_leaf_id TEXT, started_at INTEGER NOT NULL, last_seen_at INTEGER NOT NULL, ended_at INTEGER);
            CREATE TABLE task_claims (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, list_id TEXT NOT NULL, project_root TEXT NOT NULL, agent_id TEXT NOT NULL, session_id TEXT NOT NULL, session_file TEXT, session_instance_id TEXT NOT NULL, pid INTEGER NOT NULL, claim_kind TEXT NOT NULL, reason TEXT, claimed_at INTEGER NOT NULL, expires_at INTEGER, released_at INTEGER, release_reason TEXT, scope_feature_id TEXT);
            CREATE TABLE work_units (id TEXT PRIMARY KEY, task_id TEXT NOT NULL, claim_id TEXT, list_id TEXT NOT NULL, project_root TEXT NOT NULL, agent_id TEXT NOT NULL, session_id TEXT NOT NULL, session_file TEXT, session_instance_id TEXT NOT NULL, status TEXT NOT NULL, priority INTEGER NOT NULL DEFAULT 0, note TEXT, created_at INTEGER NOT NULL, dispatched_at INTEGER, completed_at INTEGER, cancelled_at INTEGER, scope_feature_id TEXT);
            CREATE TABLE visual_artifacts (id TEXT PRIMARY KEY, list_id TEXT NOT NULL, project_root TEXT NOT NULL, task_id TEXT, feature_id TEXT, work_unit_id TEXT, stage TEXT, kind TEXT NOT NULL, title TEXT NOT NULL, summary TEXT NOT NULL, path TEXT NOT NULL, mime_type TEXT NOT NULL, metadata TEXT NOT NULL DEFAULT '{}', created_by TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
            "#,
        )
        .expect("install isolated Pi Tasker schema");
}

#[tokio::test]
async fn retention_readiness_runtime_registration_survives_fresh_registry() {
    let temp = tempfile::TempDir::new().expect("runtime registration temp home");
    let _env = RuntimeRetentionEnv::new(temp.path());
    let provider: Arc<dyn Provider> = Arc::new(RuntimeCatalogProvider);
    let first_catalog = jcode_provider_core::ModelCatalogSnapshot::from_provider(provider.as_ref());
    let first = Registry::new(provider.clone()).await;
    let first_names = first.tool_names().await;

    for name in RETAINED_RUNTIME_TOOLS {
        assert!(
            first_names.iter().any(|registered| registered == name),
            "required runtime tool {name} was not registered"
        );
    }
    #[cfg(target_os = "linux")]
    assert!(
        first_names
            .iter()
            .any(|registered| registered == LINUX_RUNTIME_TOOL),
        "Linux computer-use runtime tool was not registered"
    );

    let first_definitions = retained_definition_snapshot(&first.definitions(None).await);
    for name in RETAINED_RUNTIME_TOOLS {
        assert!(
            first_definitions.contains_key(*name),
            "required runtime tool {name} has no provider definition"
        );
    }
    #[cfg(target_os = "linux")]
    assert!(
        first_definitions.contains_key(LINUX_RUNTIME_TOOL),
        "Linux computer-use runtime tool has no provider definition"
    );
    drop(first);

    let forked_provider = provider.fork();
    let second_catalog =
        jcode_provider_core::ModelCatalogSnapshot::from_provider(forked_provider.as_ref());
    assert_eq!(
        first_catalog, second_catalog,
        "provider/model catalog changed across the exact registry rebuild"
    );
    assert_eq!(
        second_catalog.provider_name.as_deref(),
        Some("runtime-catalog-provider")
    );
    assert_eq!(
        second_catalog.provider_model.as_deref(),
        Some("runtime-catalog-model")
    );
    assert_eq!(second_catalog.available_models, ["runtime-catalog-model"]);
    assert_eq!(
        second_catalog.model_routes[0].api_method,
        "runtime-catalog-api"
    );

    let second = Registry::new(forked_provider).await;
    let second_definitions = retained_definition_snapshot(&second.definitions(None).await);
    assert_eq!(
        first_definitions, second_definitions,
        "runtime capability definitions changed across a fresh registry"
    );

    for (name, definition) in &second_definitions {
        let required = required_schema_values(&definition["input_schema"]);
        assert!(
            required.contains(&"intent"),
            "runtime tool {name} lost the admission intent boundary"
        );
    }

    let mt_schema = &second_definitions["mt"]["input_schema"];
    assert!(
        mt_schema["properties"]["tasker_mode"]["enum"]
            .as_array()
            .is_some_and(|modes| modes.iter().any(|mode| mode == "plan")),
        "MetaTool registration lost its governed Tasker plan mode"
    );
    assert_eq!(
        mt_schema["properties"]["artifact_mode"]["enum"],
        serde_json::json!(["off", "apply"]),
        "MetaTool registration lost its governed artifact admission mode"
    );
    assert_eq!(
        mt_schema["properties"]["profile"]["enum"],
        serde_json::json!(["pure", "workspace-read", "workspace-mutate"]),
        "MetaTool registration lost its explicit execution profiles"
    );

    let tasker_schema = &second_definitions["tasker"]["input_schema"];
    assert!(
        tasker_schema["properties"]["action"]["enum"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|action| action == "create")),
        "Tasker registration lost its create action"
    );

    let session_search_schema = &second_definitions["session_search"]["input_schema"];
    assert_eq!(
        required_schema_values(session_search_schema),
        vec!["query", "intent"],
        "session search registration must keep its bounded query contract"
    );

    let swarm_schema = &second_definitions["swarm"]["input_schema"];
    assert!(
        swarm_schema["properties"]["action"]["enum"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|action| action == "run_plan")),
        "swarm registration lost its plan action"
    );

    let side_panel_schema = &second_definitions["side_panel"]["input_schema"];
    assert!(
        side_panel_schema["properties"]["action"]["enum"]
            .as_array()
            .is_some_and(|actions| actions.iter().any(|action| action == "status")),
        "side-panel registration lost its status action"
    );

    #[cfg(target_os = "linux")]
    {
        let linux_schema = &second_definitions[LINUX_RUNTIME_TOOL]["input_schema"];
        assert!(
            linux_schema["properties"]["action"]["description"]
                .as_str()
                .is_some_and(|description| description.contains("doctor")),
            "Linux computer-use registration lost its doctor-first contract"
        );
    }
}

#[tokio::test]
async fn retention_readiness_runtime_smoke_reopens_stateful_surfaces() {
    let temp = tempfile::TempDir::new().expect("runtime retention temp home");
    let project = temp.path().join("retention-project");
    std::fs::create_dir_all(&project).expect("runtime retention project");
    let _env = RuntimeRetentionEnv::new(temp.path());
    install_runtime_tasker_schema(&temp.path().join(".pi/tasker/tasks.db"));

    let mut prior_session =
        Session::create_with_id("runtime-retention-prior".to_string(), None, None);
    prior_session.working_dir = Some(project.to_string_lossy().into_owned());
    prior_session.add_message(
        crate::message::Role::User,
        vec![crate::message::ContentBlock::Text {
            text: "deterministic runtime retention needle".to_string(),
            cache_control: None,
        }],
    );
    prior_session
        .save()
        .expect("persist session-search fixture");

    let provider: std::sync::Arc<dyn Provider> = std::sync::Arc::new(NativeAutoCompactionProvider);
    let registry = Registry::new(provider.clone()).await;
    let context = runtime_tool_context(&project, "runtime-retention-active");

    let mt_status = registry
        .execute(
            "mt",
            serde_json::json!({
                "action": "status",
                "intent": RUNTIME_SMOKE_INTENT
            }),
            context.clone(),
        )
        .await
        .expect("MetaTool status should remain available");
    assert_eq!(mt_status.title.as_deref(), Some("MetaTool runtime status"));
    assert_eq!(
        mt_status
            .metadata
            .as_ref()
            .and_then(|metadata| metadata["experimental"].as_bool()),
        Some(true)
    );
    for profile in ["workspace-read", "workspace-mutate"] {
        let gated = registry
            .execute(
                "mt",
                serde_json::json!({
                    "action": "evaluate",
                    "code": "return 1",
                    "profile": profile,
                    "intent": RUNTIME_SMOKE_INTENT
                }),
                context.clone(),
            )
            .await
            .unwrap_err();
        assert!(
            gated
                .to_string()
                .contains("blocked until the native capability broker is implemented"),
            "MetaTool profile {profile} must remain fail-closed: {gated:#}"
        );
    }

    let task = registry
        .execute(
            "tasker",
            serde_json::json!({
                "action": "create",
                "title": "runtime retention task",
                "description": "must survive a fresh registry",
                "state": "todo",
                "intent": RUNTIME_SMOKE_INTENT
            }),
            context.clone(),
        )
        .await
        .expect("Tasker create should use the isolated canonical store");
    assert!(task.output.contains("runtime retention task"));
    let side_panel_write = registry
        .execute(
            "side_panel",
            serde_json::json!({
                "action": "write",
                "page_id": "retention-page",
                "title": "Runtime retention",
                "content": "side-panel state survives registry rebuild",
                "focus": true,
                "intent": RUNTIME_SMOKE_INTENT
            }),
            context.clone(),
        )
        .await
        .expect("side-panel write should remain registered before rebuild");
    assert!(side_panel_write.output.contains("retention-page"));

    // Exercise the host-owned admission contract directly. Calling `mt` with
    // artifact apply mode would require the external AgentOS runtime and would
    // falsely turn this retention smoke into an activation claim.
    let artifact_store = jcode_artifact_store::ArtifactStore::open_migrate(
        temp.path().join("artifact-retention/artifacts.sqlite3"),
        temp.path().join("artifact-retention/assets"),
    )
    .expect("open isolated artifact store");
    let admission = artifact_store
        .admit_bundle(jcode_artifact_store::AdmitBundleInput {
            artifact_key: "runtime-retention/artifact".to_string(),
            artifact_title: "Runtime retention artifact".to_string(),
            source_bytes: b"# Runtime retention".to_vec(),
            rendered_bytes: b"<h1>Runtime retention</h1>".to_vec(),
            annotation: Some("admitted by the host-owned smoke fixture".to_string()),
            candidate_template_key: Some("html-smoke".to_string()),
        })
        .expect("host-owned artifact admission should remain available");
    assert_eq!(admission.artifact.key, "runtime-retention/artifact");
    assert_eq!(admission.revision.number, 1);
    assert_eq!(
        admission
            .candidate
            .as_ref()
            .map(|candidate| candidate.template_key.as_str()),
        Some("html-smoke")
    );
    drop(artifact_store);
    let reopened_artifacts = jcode_artifact_store::ArtifactStore::open_migrate(
        temp.path().join("artifact-retention/artifacts.sqlite3"),
        temp.path().join("artifact-retention/assets"),
    )
    .expect("reopen isolated artifact store");
    let artifacts = reopened_artifacts
        .list_artifacts()
        .expect("list retained artifacts");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].key, "runtime-retention/artifact");
    drop(reopened_artifacts);

    drop(registry);

    let reloaded = Registry::new(provider).await;
    let tasker_status = reloaded
        .execute(
            "tasker",
            serde_json::json!({
                "action": "status",
                "limit": 1,
                "intent": RUNTIME_SMOKE_INTENT
            }),
            context.clone(),
        )
        .await
        .expect("Tasker status should reopen the isolated store");
    let tasker_metadata = tasker_status.metadata.expect("Tasker status metadata");
    assert_eq!(tasker_metadata["counts"]["tasks"], 1);
    assert_eq!(
        tasker_metadata["list_meta"]["projectRoot"],
        project.to_string_lossy().as_ref()
    );

    let session_search = reloaded
        .execute(
            "session_search",
            serde_json::json!({
                "query": "runtime retention needle",
                "limit": 1,
                "max_per_session": 1,
                "max_scan_sessions": 10,
                "include_external": false,
                "intent": RUNTIME_SMOKE_INTENT
            }),
            context.clone(),
        )
        .await
        .expect("session search should reopen persisted sessions");
    assert!(
        session_search.output.contains("runtime retention needle"),
        "session search did not retain the persisted transcript: {}",
        session_search.output
    );

    let side_panel = reloaded
        .execute(
            "side_panel",
            serde_json::json!({
                "action": "status",
                "intent": RUNTIME_SMOKE_INTENT
            }),
            context.clone(),
        )
        .await
        .expect("side-panel status should remain registered after rebuild");
    assert_eq!(side_panel.title.as_deref(), Some("side_panel"));
    assert!(
        side_panel.output.contains("retention-page"),
        "side-panel state did not survive the registry rebuild: {}",
        side_panel.output
    );

    #[cfg(target_os = "linux")]
    {
        let linux = reloaded
            .execute(
                LINUX_RUNTIME_TOOL,
                serde_json::json!({
                    "action": "discover",
                    "intent": RUNTIME_SMOKE_INTENT
                }),
                context,
            )
            .await
            .expect("Linux computer-use discover should be local and bounded");
        assert!(linux.output.contains("linux_computer_use actions"));
    }

    // Swarm is intentionally registration/schema-only here. Its status action
    // is server-backed, so invoking it would turn this admission smoke into a
    // live-daemon dependency.
    assert!(
        reloaded
            .tool_names()
            .await
            .iter()
            .any(|name| name == "swarm")
    );
}
