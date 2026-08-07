use crate::provider::Provider;
use crate::session::Session;
use crate::tool::{CandidateLaneHost, CandidateLaneHostRequest, CandidateLaneHostResponse};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use jcode_agent_runtime::InterruptSignal;
use jcode_tasker_orchestration::{
    AcceptanceContract, CandidateAgentResult, CandidateExecutor, CandidateExecutorError,
    CandidateLaneExecutionRequest, CandidateLaneHostContext, CandidateLaneLaunchRequest,
    CandidateOrchestrator, ProvenanceTemplate, ValidationCommand,
};
use jcode_tasker_pi::{PiTaskerStore, ProjectPartition};
use jcode_tasker_types::{ProjectId, TaskId};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex as StdMutex};
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::{SessionAgents, SessionInterruptQueues, SwarmMember, VersionedPlan};

const CHILD_TERMINATION_TIMEOUT: Duration = Duration::from_secs(10);
const ALLOWED_VALIDATOR_PROGRAMS: &[&str] = &[
    "cargo",
    "cargo-nextest",
    "go",
    "just",
    "make",
    "npm",
    "pnpm",
    "pytest",
    "python",
    "python3",
    "swift",
    "true",
    "yarn",
];

#[derive(Clone)]
struct ActiveCandidateCancellation {
    session_id: String,
    token: CancellationToken,
}

static ACTIVE_CANDIDATE_CANCELLATIONS: LazyLock<
    StdMutex<HashMap<String, ActiveCandidateCancellation>>,
> = LazyLock::new(|| StdMutex::new(HashMap::new()));

fn register_cancellation(operation_id: &str, session_id: &str, cancellation: &CancellationToken) {
    if let Ok(mut active) = ACTIVE_CANDIDATE_CANCELLATIONS.lock() {
        active.insert(
            operation_id.to_string(),
            ActiveCandidateCancellation {
                session_id: session_id.to_string(),
                token: cancellation.clone(),
            },
        );
    }
}

fn unregister_cancellation(operation_id: &str) {
    if let Ok(mut active) = ACTIVE_CANDIDATE_CANCELLATIONS.lock() {
        active.remove(operation_id);
    }
}

pub(super) fn cancel_operation(session_id: &str, operation_id: &str) -> bool {
    ACTIVE_CANDIDATE_CANCELLATIONS
        .lock()
        .ok()
        .and_then(|active| active.get(operation_id).cloned())
        .filter(|active| active.session_id == session_id)
        .map(|active| {
            active.token.cancel();
            true
        })
        .unwrap_or(false)
}

/// Host-routed candidate job. The guest can propose only the declarative
/// request; this job keeps the caller identity and transport claims separate
/// until the server resolves them against live state.
#[derive(Debug, Clone)]
pub(super) struct CandidateLaneJob {
    pub operation_id: String,
    pub session_id: String,
    pub working_dir_claim: PathBuf,
    pub expected_snapshot_hash: String,
    pub proposal: CandidateLaneLaunchRequest,
}

impl CandidateLaneJob {
    fn from_request(request: CandidateLaneHostRequest) -> Result<Self> {
        if request.operation_id.trim().is_empty() {
            bail!("candidate authority rejected: operation id is required");
        }
        if request.session_id.trim().is_empty() {
            bail!("candidate authority rejected: session id is required");
        }
        if request.working_dir.trim().is_empty() {
            bail!("candidate authority rejected: working directory is required");
        }
        if request.expected_snapshot_hash.trim().is_empty() {
            bail!("candidate authority rejected: snapshot identity is required");
        }
        if request.receipt_id.trim().is_empty() {
            bail!("candidate authority rejected: Tasker plan receipt is required");
        }
        let proposal: CandidateLaneLaunchRequest =
            serde_json::from_value(request.proposal).context("decode guest candidate proposal")?;
        proposal
            .validate()
            .map_err(|error| anyhow!("candidate proposal rejected: {error}"))?;
        Ok(Self {
            operation_id: request.operation_id,
            session_id: request.session_id,
            working_dir_claim: PathBuf::from(request.working_dir),
            expected_snapshot_hash: request.expected_snapshot_hash,
            proposal,
        })
    }
}

#[derive(Debug, Clone)]
struct ResolvedCandidateLaneJob {
    job: CandidateLaneJob,
    project_root: PathBuf,
    project_id: ProjectId,
    task_id: TaskId,
    base_commit: String,
    expected_revision: u64,
    provenance: ProvenanceTemplate,
    validation_commands: Vec<ValidationCommand>,
    model: Option<String>,
    provider_key: Option<String>,
    route_api_method: Option<String>,
    effort: Option<String>,
    worktree_root: PathBuf,
}

#[derive(Clone)]
pub(crate) struct ServerCandidateLaneHost {
    provider_template: Arc<dyn Provider>,
    sessions: SessionAgents,
    global_session_id: Arc<RwLock<String>>,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: Arc<RwLock<HashMap<String, std::collections::HashSet<String>>>>,
    swarm_coordinators: Arc<RwLock<HashMap<String, String>>>,
    swarm_plans: Arc<RwLock<HashMap<String, VersionedPlan>>>,
    soft_interrupt_queues: SessionInterruptQueues,
    mcp_pool: Arc<crate::mcp::SharedMcpPool>,
    database_path: PathBuf,
}

impl ServerCandidateLaneHost {
    #[expect(
        clippy::too_many_arguments,
        reason = "host wiring must explicitly bind the server-owned session and swarm state"
    )]
    pub(crate) fn new(
        provider_template: Arc<dyn Provider>,
        sessions: SessionAgents,
        global_session_id: Arc<RwLock<String>>,
        swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
        swarms_by_id: Arc<RwLock<HashMap<String, std::collections::HashSet<String>>>>,
        swarm_coordinators: Arc<RwLock<HashMap<String, String>>>,
        swarm_plans: Arc<RwLock<HashMap<String, VersionedPlan>>>,
        soft_interrupt_queues: SessionInterruptQueues,
        mcp_pool: Arc<crate::mcp::SharedMcpPool>,
    ) -> Self {
        Self {
            provider_template,
            sessions,
            global_session_id,
            swarm_members,
            swarms_by_id,
            swarm_coordinators,
            swarm_plans,
            soft_interrupt_queues,
            mcp_pool,
            database_path: jcode_tasker_pi::default_db_path(),
        }
    }

    pub(crate) async fn execute_request(
        &self,
        request: CandidateLaneHostRequest,
        caller_cancellation: CancellationToken,
    ) -> Result<CandidateLaneHostResponse> {
        let job = CandidateLaneJob::from_request(request)?;
        let operation_id = job.operation_id.clone();
        let execution_cancellation = CancellationToken::new();
        register_cancellation(&operation_id, &job.session_id, &execution_cancellation);
        let relay = {
            let execution_cancellation = execution_cancellation.clone();
            tokio::spawn(async move {
                caller_cancellation.cancelled().await;
                execution_cancellation.cancel();
            })
        };
        let result = self.execute_resolved_job(job, execution_cancellation).await;
        relay.abort();
        unregister_cancellation(&operation_id);
        result
    }

    async fn resolve_job(&self, job: CandidateLaneJob) -> Result<ResolvedCandidateLaneJob> {
        let session = Session::load_startup_stub(&job.session_id)
            .or_else(|_| Session::load(&job.session_id))
            .context("load originating session authority")?;
        if !self.sessions.read().await.contains_key(&job.session_id) {
            bail!(
                "candidate authority rejected: originating session {} is not live",
                job.session_id
            );
        }
        let authoritative_dir = session
            .working_dir
            .as_deref()
            .filter(|dir| !dir.trim().is_empty())
            .ok_or_else(|| anyhow!("candidate authority rejected: session has no project root"))?;
        let authoritative_dir = canonical_project_root(Path::new(authoritative_dir))?;
        let claimed_dir = canonical_project_root(&job.working_dir_claim)?;
        if claimed_dir != authoritative_dir {
            bail!(
                "candidate authority rejected: working directory claim does not match the live session"
            );
        }

        let store = PiTaskerStore::open(ProjectPartition::with_db_path(
            self.database_path.clone(),
            authoritative_dir.to_string_lossy().into_owned(),
        ))
        .context("open canonical Tasker project")?;
        let project_id: ProjectId = store
            .partition()
            .list_id
            .parse()
            .map_err(|error| anyhow!("canonical Tasker project id is invalid: {error}"))?;
        let (_, snapshot_hash) =
            crate::tool::tasker_snapshot_for_project(&store, &project_id.to_string())?;
        if snapshot_hash != job.expected_snapshot_hash {
            bail!("candidate authority rejected: Tasker snapshot changed before execution");
        }

        let task_reference = job
            .proposal
            .task_reference
            .as_deref()
            .filter(|reference| !reference.trim().is_empty())
            .ok_or_else(|| anyhow!("candidate authority rejected: task reference is required"))?;
        let task_id: TaskId = store
            .resolve_task_id(task_reference)?
            .ok_or_else(|| anyhow!("candidate authority rejected: task reference not found"))?
            .parse()
            .map_err(|error| anyhow!("resolved Tasker task id is invalid: {error}"))?;

        let git = jcode_tasker_git::GitCandidateAdapter::try_new(&authoritative_dir)
            .context("open canonical candidate Git repository")?;
        let base_commit = git.resolve_commit("HEAD")?;
        let concurrency = store.open_concurrency_store()?;
        let expected_revision = concurrency.current_revision(&project_id.to_string())?;
        let validation_commands = resolve_validation_commands(&job.proposal.acceptance)?;
        let mut provenance = ProvenanceTemplate::new(
            job.session_id.clone(),
            session
                .short_name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| job.session_id.clone()),
        );
        provenance.model_id = session.model.clone();

        Ok(ResolvedCandidateLaneJob {
            job,
            project_root: authoritative_dir.clone(),
            project_id,
            task_id,
            base_commit,
            expected_revision,
            provenance,
            validation_commands,
            model: session.model,
            provider_key: session.provider_key,
            route_api_method: session.route_api_method,
            effort: session.reasoning_effort,
            worktree_root: authoritative_dir
                .join(".jcode")
                .join("tasker-candidate-lanes"),
        })
    }

    async fn execute_resolved_job(
        &self,
        job: CandidateLaneJob,
        cancellation: CancellationToken,
    ) -> Result<CandidateLaneHostResponse> {
        let resolved = self.resolve_job(job).await?;
        let host = self.clone();
        tokio::task::spawn_blocking(move || -> Result<CandidateLaneHostResponse> {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("build candidate execution runtime")?;
            runtime.block_on(async move {
                let partition = ProjectPartition::with_db_path(
                    host.database_path.clone(),
                    resolved.project_root.to_string_lossy().into_owned(),
                );
                let store =
                    PiTaskerStore::open(partition).context("reopen canonical Tasker project")?;
                let concurrency = store.open_concurrency_store()?;
                let git = jcode_tasker_git::GitCandidateAdapter::try_new(&resolved.project_root)?;
                let mut orchestrator = CandidateOrchestrator::new(concurrency, git)?;
                let executor = ServerCandidateExecutor::new(host, &resolved);
                let host_context = CandidateLaneHostContext {
                    project_id: resolved.project_id,
                    task_id: resolved.task_id,
                    base_commit: resolved.base_commit,
                    provenance: resolved.provenance,
                    expected_revision: resolved.expected_revision,
                    validation_commands: resolved.validation_commands,
                };
                let bound = resolved.job.proposal.bind(host_context)?;
                let report = orchestrator
                    .execute_candidate_set(bound, resolved.worktree_root, &executor, cancellation)
                    .await
                    .map_err(|error| anyhow!("candidate orchestration failed: {error}"))?;
                let status = if report.submitted_count() > 0 {
                    "completed"
                } else {
                    "failed"
                };
                Ok(CandidateLaneHostResponse {
                    status: status.to_string(),
                    redacted: true,
                    report: report.as_value(),
                })
            })
        })
        .await
        .context("join candidate orchestration runtime")?
    }
}

#[async_trait]
impl CandidateLaneHost for ServerCandidateLaneHost {
    async fn execute_candidate_lanes(
        &self,
        request: CandidateLaneHostRequest,
        cancellation: CancellationToken,
    ) -> Result<CandidateLaneHostResponse> {
        self.execute_request(request, cancellation).await
    }
}

fn canonical_project_root(path: &Path) -> Result<PathBuf> {
    let canonical = std::fs::canonicalize(path)
        .with_context(|| format!("canonicalize candidate project root {}", path.display()))?;
    if !canonical.is_dir() {
        bail!(
            "candidate project root is not a directory: {}",
            canonical.display()
        );
    }
    Ok(canonical)
}

fn resolve_validation_commands(acceptance: &AcceptanceContract) -> Result<Vec<ValidationCommand>> {
    if acceptance.validation_commands.is_empty() {
        bail!("candidate authority rejected: validation commands are required");
    }
    for command in &acceptance.validation_commands {
        if !ALLOWED_VALIDATOR_PROGRAMS.contains(&command.program.as_str()) {
            bail!(
                "candidate authority rejected: validator {} is not host-allowlisted",
                command.program
            );
        }
        if command.args.iter().any(|arg| {
            arg.contains("&&")
                || arg.contains("||")
                || arg.contains(';')
                || arg.contains('|')
                || arg.contains('>')
                || arg.contains('<')
        }) {
            bail!("candidate authority rejected: validator arguments contain shell metacharacters");
        }
    }
    Ok(acceptance.validation_commands.clone())
}

#[derive(Clone)]
struct ServerCandidateExecutor {
    host: ServerCandidateLaneHost,
    origin_session_id: String,
    model: Option<String>,
    provider_key: Option<String>,
    route_api_method: Option<String>,
    effort: Option<String>,
    active: Arc<Mutex<HashMap<String, ChildLaneControl>>>,
}

#[derive(Clone)]
struct ChildLaneControl {
    shutdown: InterruptSignal,
}

impl ServerCandidateExecutor {
    fn new(host: ServerCandidateLaneHost, resolved: &ResolvedCandidateLaneJob) -> Self {
        Self {
            host,
            origin_session_id: resolved.job.session_id.clone(),
            model: resolved.model.clone(),
            provider_key: resolved.provider_key.clone(),
            route_api_method: resolved.route_api_method.clone(),
            effort: resolved.effort.clone(),
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn finish_child(&self, candidate_id: String, session_id: &str) {
        self.active.lock().await.remove(&candidate_id);
        if let Some(agent) = self.host.sessions.read().await.get(session_id).cloned() {
            let mut agent = agent.lock().await;
            agent.mark_closed();
        }
        let _ = super::remove_session_entry(&self.host.sessions, session_id).await;
        super::remove_session_interrupt_queue(&self.host.soft_interrupt_queues, session_id).await;
    }

    async fn parse_result(output: String) -> Result<CandidateAgentResult, CandidateExecutorError> {
        let trimmed = output.trim();
        if trimmed.is_empty() {
            return Err(CandidateExecutorError::Failed(
                "candidate returned no structured completion".into(),
            ));
        }
        let result: CandidateAgentResult = serde_json::from_str(trimmed).map_err(|error| {
            CandidateExecutorError::Failed(format!(
                "candidate completion must be one JSON object: {error}"
            ))
        })?;
        if result
            .summary
            .as_deref()
            .is_none_or(|summary| summary.trim().is_empty())
        {
            return Err(CandidateExecutorError::Failed(
                "candidate completion requires a non-empty summary".into(),
            ));
        }
        Ok(result)
    }
}

#[async_trait]
impl CandidateExecutor for ServerCandidateExecutor {
    async fn execute(
        &self,
        request: CandidateLaneExecutionRequest,
        cancellation: CancellationToken,
    ) -> std::result::Result<CandidateAgentResult, CandidateExecutorError> {
        let command = format!("create_session:{}", request.lane.worktree.path.display());
        let raw_session = super::headless::create_headless_session(
            &self.host.sessions,
            &self.host.global_session_id,
            &self.host.provider_template,
            &command,
            &self.host.swarm_members,
            &self.host.swarms_by_id,
            &self.host.swarm_coordinators,
            &self.host.swarm_plans,
            &self.host.soft_interrupt_queues,
            false,
            self.model.clone(),
            self.provider_key.clone(),
            self.route_api_method.clone(),
            self.effort.clone(),
            Some(Arc::clone(&self.host.mcp_pool)),
            Some(self.origin_session_id.clone()),
        )
        .await
        .map_err(|error| CandidateExecutorError::Failed(error.to_string()))?;
        let session_id = serde_json::from_str::<Value>(&raw_session)
            .ok()
            .and_then(|value| value["session_id"].as_str().map(ToOwned::to_owned))
            .ok_or_else(|| {
                CandidateExecutorError::Failed("headless child did not return a session id".into())
            })?;
        let agent = self
            .host
            .sessions
            .read()
            .await
            .get(&session_id)
            .cloned()
            .ok_or_else(|| {
                CandidateExecutorError::Failed("headless child was not registered".into())
            })?;
        let shutdown = agent.lock().await.graceful_shutdown_signal();
        self.active.lock().await.insert(
            request.lane.candidate_id.to_string(),
            ChildLaneControl {
                shutdown: shutdown.clone(),
            },
        );

        let mut turn = Box::pin(async {
            let mut agent = agent.lock().await;
            agent.run_once_capture(&request.prompt).await
        });
        let result = tokio::select! {
            output = &mut turn => {
                match output {
                    Ok(output) => Self::parse_result(output).await,
                    Err(error) => Err(CandidateExecutorError::Failed(error.to_string())),
                }
            }
            _ = cancellation.cancelled() => {
                shutdown.fire();
                match timeout(CHILD_TERMINATION_TIMEOUT, &mut turn).await {
                    Ok(_) => Err(CandidateExecutorError::Cancelled),
                    Err(_) => Err(CandidateExecutorError::Failed(
                        "headless child cancellation was not acknowledged".into(),
                    )),
                }
            }
        };
        let termination_failed = matches!(
            &result,
            Err(CandidateExecutorError::Failed(reason)) if reason.contains("not acknowledged")
        );
        if !termination_failed {
            self.finish_child(request.lane.candidate_id.to_string(), &session_id)
                .await;
        }
        result
    }

    async fn cancel(
        &self,
        request: &CandidateLaneExecutionRequest,
    ) -> std::result::Result<(), CandidateExecutorError> {
        let active = self.active.lock().await;
        let control = active
            .get(&request.lane.candidate_id.to_string())
            .ok_or_else(|| {
                CandidateExecutorError::Failed("candidate child is not active".into())
            })?;
        control.shutdown.fire();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn candidate_job_rejects_authority_spoofing() {
        let proposal = json!({
            "policy": {"kind": "speculative", "max_candidates": 2},
            "laneCount": 2,
            "acceptance": {
                "validationCommands": [{"program": "true", "args": []}],
                "acceptanceCriteria": "pass",
                "resourceIntents": []
            },
            "prompt": "work",
            "limits": {"maxLanes": 2, "laneTimeoutMs": 1000},
            "projectId": "guest-project",
        });
        let request = CandidateLaneHostRequest {
            id: 1,
            operation_id: "op-1".into(),
            session_id: "session-1".into(),
            working_dir: ".".into(),
            expected_snapshot_hash: "sha256:host".into(),
            receipt_id: "tpr_1".into(),
            proposal,
        };
        let error = CandidateLaneJob::from_request(request).expect_err("spoof must reject");
        assert!(
            error
                .to_string()
                .contains("decode guest candidate proposal")
        );
    }

    #[test]
    fn validator_policy_rejects_unsafe_programs() {
        let acceptance = AcceptanceContract::new(
            vec![ValidationCommand::new("rm", ["-rf", "."])],
            "pass",
            Vec::new(),
        );
        let error = resolve_validation_commands(&acceptance).expect_err("unsafe validator");
        assert!(error.to_string().contains("not host-allowlisted"));
    }

    #[test]
    fn cancellation_rejects_a_different_originating_session() {
        let cancellation = CancellationToken::new();
        register_cancellation("candidate-op-authority", "session-a", &cancellation);
        assert!(!cancel_operation("session-b", "candidate-op-authority"));
        assert!(!cancellation.is_cancelled());
        assert!(cancel_operation("session-a", "candidate-op-authority"));
        assert!(cancellation.is_cancelled());
        unregister_cancellation("candidate-op-authority");
    }
}
