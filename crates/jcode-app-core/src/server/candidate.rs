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

fn register_cancellation(
    operation_id: &str,
    session_id: &str,
    cancellation: &CancellationToken,
) -> bool {
    let Ok(mut active) = ACTIVE_CANDIDATE_CANCELLATIONS.lock() else {
        return false;
    };
    if active.contains_key(operation_id) {
        return false;
    }
    active.insert(
        operation_id.to_string(),
        ActiveCandidateCancellation {
            session_id: session_id.to_string(),
            token: cancellation.clone(),
        },
    );
    true
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
        if !register_cancellation(&operation_id, &job.session_id, &execution_cancellation) {
            bail!(
                "candidate execution operation is already active or cancellation registry is unavailable"
            );
        }
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
        let pi_list_id = store.partition().list_id.clone();
        let project_id = native_project_id_for_pi_list(&pi_list_id);
        let (_, snapshot_hash) = crate::tool::tasker_snapshot_for_project(&store, &pi_list_id)?;
        if snapshot_hash != job.expected_snapshot_hash {
            bail!("candidate authority rejected: Tasker snapshot changed before execution");
        }

        let task_reference = job
            .proposal
            .task_reference
            .as_deref()
            .filter(|reference| !reference.trim().is_empty())
            .ok_or_else(|| anyhow!("candidate authority rejected: task reference is required"))?;
        let pi_task_id = store
            .resolve_task_id(task_reference)?
            .ok_or_else(|| anyhow!("candidate authority rejected: task reference not found"))?;
        let task_id = native_task_id_for_pi_task(&pi_task_id);

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

fn native_project_id_for_pi_list(list_id: &str) -> ProjectId {
    ProjectId::from_uuid(uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        format!("jcode-tasker-pi-list:{list_id}").as_bytes(),
    ))
}

fn native_task_id_for_pi_task(task_id: &str) -> TaskId {
    TaskId::from_uuid(uuid::Uuid::new_v5(
        &uuid::Uuid::NAMESPACE_OID,
        format!("jcode-tasker-pi-task:{task_id}").as_bytes(),
    ))
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
    session_id: String,
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
        let removed_agent = super::remove_session_entry(&self.host.sessions, session_id).await;
        if let Some(agent) = removed_agent
            && let Ok(mut agent) = agent.try_lock()
        {
            agent.mark_closed();
        }
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
                session_id: session_id.clone(),
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
        let control = {
            let active = self.active.lock().await;
            active
                .get(&request.lane.candidate_id.to_string())
                .cloned()
                .ok_or_else(|| {
                    CandidateExecutorError::Failed("candidate child is not active".into())
                })?
        };
        control.shutdown.fire();
        self.finish_child(request.lane.candidate_id.to_string(), &control.session_id)
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Agent;
    use crate::message::{ContentBlock, Message, StreamEvent, ToolDefinition};
    use crate::provider::{EventStream, Provider};
    use crate::session::Session;
    use crate::tool::Registry;
    use chrono::Utc;
    use futures::stream;
    use jcode_tasker_pi::{CreateTask, NoteInput};
    use jcode_tasker_promotion::PromotionRequest;
    use jcode_tasker_rounds::RoundCompletion;
    use jcode_tasker_types::{
        AdjudicationBallot, BallotId, CandidateAssessment, CandidateId, ValidatorIdentity,
    };
    use rusqlite::{Connection, TransactionBehavior, params};
    use std::collections::HashSet;
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::{TempDir, tempdir};

    use serde_json::json;

    const CANONICAL_REF: &str = "refs/heads/main";

    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        _home: TempDir,
        previous_home: Option<std::ffi::OsString>,
        previous_jcode_home: Option<std::ffi::OsString>,
    }

    impl EnvGuard {
        fn new() -> Self {
            let lock = crate::storage::lock_test_env();
            let home = tempdir().expect("create isolated home");
            let previous_home = std::env::var_os("HOME");
            let previous_jcode_home = std::env::var_os("JCODE_HOME");
            crate::env::set_var("HOME", home.path());
            crate::env::set_var("JCODE_HOME", home.path().join(".jcode"));
            Self {
                _lock: lock,
                _home: home,
                previous_home,
                previous_jcode_home,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous_home {
                Some(value) => crate::env::set_var("HOME", value),
                None => crate::env::remove_var("HOME"),
            }
            match &self.previous_jcode_home {
                Some(value) => crate::env::set_var("JCODE_HOME", value),
                None => crate::env::remove_var("JCODE_HOME"),
            }
        }
    }

    #[derive(Default)]
    struct HeadlessDogfoodState {
        calls: AtomicUsize,
        active: AtomicUsize,
        max_active: AtomicUsize,
    }

    #[derive(Clone)]
    struct HeadlessDogfoodProvider {
        state: Arc<HeadlessDogfoodState>,
        barrier: Option<Arc<tokio::sync::Barrier>>,
        hang_after_start: bool,
    }

    impl HeadlessDogfoodProvider {
        fn new(lanes: usize) -> (Self, Arc<HeadlessDogfoodState>) {
            let state = Arc::new(HeadlessDogfoodState::default());
            (
                Self {
                    state: Arc::clone(&state),
                    barrier: Some(Arc::new(tokio::sync::Barrier::new(lanes))),
                    hang_after_start: false,
                },
                state,
            )
        }

        fn hanging() -> (Self, Arc<HeadlessDogfoodState>) {
            let state = Arc::new(HeadlessDogfoodState::default());
            (
                Self {
                    state: Arc::clone(&state),
                    barrier: None,
                    hang_after_start: true,
                },
                state,
            )
        }

        fn record_max(&self, current: usize) {
            let mut observed = self.state.max_active.load(Ordering::SeqCst);
            while current > observed {
                match self.state.max_active.compare_exchange(
                    observed,
                    current,
                    Ordering::SeqCst,
                    Ordering::SeqCst,
                ) {
                    Ok(_) => break,
                    Err(next) => observed = next,
                }
            }
        }
    }

    #[async_trait]
    impl Provider for HeadlessDogfoodProvider {
        async fn complete(
            &self,
            messages: &[Message],
            _tools: &[ToolDefinition],
            _system: &str,
            _resume_session_id: Option<&str>,
        ) -> Result<EventStream> {
            let prompt = latest_user_text(messages).context("candidate prompt is present")?;
            let worktree = text_between(&prompt, "Work only in `", "`")
                .context("candidate prompt includes worktree")?;
            let candidate_id = text_between(&prompt, "Candidate id: `", "`")
                .context("candidate prompt includes candidate id")?;
            self.state.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.state.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.record_max(active);
            if self.hang_after_start {
                return Ok(Box::pin(stream::pending::<Result<StreamEvent>>()));
            }
            if let Some(barrier) = &self.barrier {
                barrier.wait().await;
            }

            let worktree = PathBuf::from(worktree);
            std::fs::write(
                worktree.join("candidate-output.txt"),
                format!("headless candidate {candidate_id}\n"),
            )
            .context("write candidate output")?;
            git(&worktree, &["config", "user.name", "Headless Candidate"]);
            git(
                &worktree,
                &["config", "user.email", "headless-candidate@example.invalid"],
            );
            git(&worktree, &["add", "candidate-output.txt"]);
            git(
                &worktree,
                &[
                    "commit",
                    "-m",
                    &format!("headless candidate {candidate_id}"),
                ],
            );
            self.state.active.fetch_sub(1, Ordering::SeqCst);

            let response = json!({
                "summary": format!("headless child {candidate_id} committed"),
                "advisoryEvidence": [{"kind": "untrusted-provider-claim"}]
            })
            .to_string();
            Ok(Box::pin(stream::iter(vec![
                Ok(StreamEvent::TextDelta(response)),
                Ok(StreamEvent::MessageEnd { stop_reason: None }),
            ])))
        }

        fn name(&self) -> &str {
            "headless-dogfood"
        }

        fn model(&self) -> String {
            "headless-dogfood-model".to_string()
        }

        fn fork(&self) -> Arc<dyn Provider> {
            Arc::new(self.clone())
        }
    }

    struct DogfoodFixture {
        _env: EnvGuard,
        _workspace: TempDir,
        repository: PathBuf,
        database: PathBuf,
        provider_state: Arc<HeadlessDogfoodState>,
        host: ServerCandidateLaneHost,
        sessions: SessionAgents,
        task_display_id: i64,
        snapshot_hash: String,
        project_id: ProjectId,
        task_id: TaskId,
        base_commit: String,
        origin_session_id: String,
    }

    impl DogfoodFixture {
        async fn new(lanes: usize) -> Self {
            Self::with_provider(HeadlessDogfoodProvider::new(lanes)).await
        }

        async fn hanging() -> Self {
            Self::with_provider(HeadlessDogfoodProvider::hanging()).await
        }

        async fn with_provider(
            provider_fixture: (HeadlessDogfoodProvider, Arc<HeadlessDogfoodState>),
        ) -> Self {
            let env = EnvGuard::new();
            let workspace = tempdir().expect("create dogfood workspace");
            let repository = workspace.path().join("repo");
            std::fs::create_dir_all(&repository).expect("create disposable repo");
            git(&repository, &["init", "-b", "main"]);
            git(&repository, &["config", "user.name", "Tasker Dogfood"]);
            git(
                &repository,
                &["config", "user.email", "tasker-dogfood@example.invalid"],
            );
            std::fs::write(repository.join("README.md"), "base\n").expect("write base file");
            git(&repository, &["add", "README.md"]);
            git(&repository, &["commit", "-m", "base"]);
            let base_commit = git(&repository, &["rev-parse", CANONICAL_REF]);

            let database = workspace.path().join("tasks.db");
            install_pi_schema(&database);
            let partition = ProjectPartition::with_db_path(
                &database,
                repository.to_string_lossy().into_owned(),
            );
            let mut store = PiTaskerStore::open(partition).expect("open isolated Tasker store");
            let task = store
                .create_task(CreateTask {
                    title: "Headless candidate dogfood".to_string(),
                    description: Some("Exercise real headless candidate lanes".to_string()),
                    state: Some("todo".to_string()),
                    feature_id: None,
                    indexes: None,
                    depends_on: Vec::new(),
                    notes: Vec::<NoteInput>::new(),
                })
                .expect("create isolated task");
            let pi_list_id = store.partition().list_id.clone();
            let project_id = native_project_id_for_pi_list(&pi_list_id);
            let task_id = native_task_id_for_pi_task(&task.id);
            let (_, snapshot_hash) = crate::tool::tasker_snapshot_for_project(&store, &pi_list_id)
                .expect("snapshot isolated Tasker store");
            drop(store);

            let (provider, provider_state) = provider_fixture;
            let provider: Arc<dyn Provider> = Arc::new(provider);
            let registry = Registry::new(Arc::clone(&provider)).await;
            let origin_session_id = "session_headless_candidate_origin".to_string();
            let mut origin_session = Session::create_with_id(origin_session_id.clone(), None, None);
            origin_session.working_dir = Some(repository.to_string_lossy().into_owned());
            origin_session.short_name = Some("origin-dogfood".to_string());
            origin_session.model = Some("headless-dogfood-model".to_string());
            origin_session
                .save()
                .expect("persist origin authority session");
            let origin_agent = Arc::new(Mutex::new(Agent::new_with_session(
                Arc::clone(&provider),
                registry,
                origin_session,
                None,
            )));

            let sessions: SessionAgents = Arc::new(RwLock::new(HashMap::from([(
                origin_session_id.clone(),
                origin_agent,
            )])));
            let global_session_id = Arc::new(RwLock::new(origin_session_id.clone()));
            let swarm_members = Arc::new(RwLock::new(HashMap::new()));
            let swarms_by_id = Arc::new(RwLock::new(HashMap::<String, HashSet<String>>::new()));
            let swarm_coordinators = Arc::new(RwLock::new(HashMap::new()));
            let swarm_plans = Arc::new(RwLock::new(HashMap::new()));
            let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
            let mcp_pool = Arc::new(crate::mcp::SharedMcpPool::from_default_config());
            let host = ServerCandidateLaneHost {
                provider_template: provider,
                sessions: Arc::clone(&sessions),
                global_session_id,
                swarm_members,
                swarms_by_id,
                swarm_coordinators,
                swarm_plans,
                soft_interrupt_queues,
                mcp_pool,
                database_path: database.clone(),
            };

            Self {
                _env: env,
                _workspace: workspace,
                repository,
                database,
                provider_state,
                host,
                sessions,
                task_display_id: task.display_id,
                snapshot_hash,
                project_id,
                task_id,
                base_commit,
                origin_session_id,
            }
        }

        fn request(
            &self,
            operation_id: &str,
            policy: serde_json::Value,
            lanes: u16,
        ) -> CandidateLaneHostRequest {
            CandidateLaneHostRequest {
                id: 42,
                operation_id: operation_id.to_string(),
                session_id: self.origin_session_id.clone(),
                working_dir: self.repository.to_string_lossy().into_owned(),
                expected_snapshot_hash: self.snapshot_hash.clone(),
                receipt_id: format!("receipt-{operation_id}"),
                proposal: json!({
                    "taskReference": format!("#{}", self.task_display_id),
                    "policy": policy,
                    "laneCount": lanes,
                    "acceptance": {
                        "validationCommands": [{"program": "true", "args": []}],
                        "acceptanceCriteria": "headless candidate must commit and pass host validation",
                        "resourceIntents": [{
                            "kind": "file",
                            "selector": "candidate-output.txt",
                            "access": "propose_write",
                            "rationale": "candidate implementation output"
                        }]
                    },
                    "prompt": "Deterministically write and commit candidate-output.txt, then return the required JSON.",
                    "limits": {"maxLanes": lanes, "laneTimeoutMs": 15000}
                }),
            }
        }
        fn mark_candidates_eligible(
            &self,
            candidate_ids: &[CandidateId],
            expected_revision: u64,
        ) -> u64 {
            let mut connection =
                Connection::open(&self.database).expect("open eligibility database");
            connection
                .busy_timeout(Duration::from_secs(30))
                .expect("set eligibility busy timeout");
            let transaction = connection
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .expect("begin eligibility transaction");
            let actual: i64 = transaction
                .query_row(
                    "SELECT revision FROM concurrency_project_revisions WHERE project_id = ?1",
                    [self.project_id.to_string()],
                    |row| row.get(0),
                )
                .expect("read eligibility revision");
            assert_eq!(
                u64::try_from(actual).expect("non-negative revision"),
                expected_revision,
                "eligibility transition must use the host-reported revision"
            );

            for candidate_id in candidate_ids {
                let changed = transaction
                    .execute(
                        "UPDATE candidates SET state = 'eligible', updated_at = ?1 WHERE id = ?2",
                        params![Utc::now().to_rfc3339(), candidate_id.to_string()],
                    )
                    .expect("mark candidate eligible");
                assert_eq!(changed, 1, "candidate must exist before validation");
            }
            let changed = transaction
                .execute(
                    "UPDATE concurrency_project_revisions
                     SET revision = revision + 1, updated_at = ?1
                     WHERE project_id = ?2 AND revision = ?3",
                    params![
                        Utc::now().to_rfc3339(),
                        self.project_id.to_string(),
                        i64::try_from(expected_revision).expect("revision fits SQLite")
                    ],
                )
                .expect("bump validation revision");
            assert_eq!(changed, 1, "eligibility must use the expected revision");
            transaction.commit().expect("commit eligibility batch");
            expected_revision + 1
        }
    }

    async fn wait_for_provider_calls(state: &HeadlessDogfoodState, expected: usize) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            if state.calls.load(Ordering::SeqCst) >= expected {
                return;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {expected} provider call(s)"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    fn latest_user_text(messages: &[Message]) -> Option<String> {
        messages.iter().rev().find_map(|message| {
            message.content.iter().find_map(|block| match block {
                ContentBlock::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
        })
    }

    fn text_between(text: &str, prefix: &str, suffix: &str) -> Option<String> {
        let start = text.find(prefix)? + prefix.len();
        let tail = &text[start..];
        let end = tail.find(suffix)?;
        Some(tail[..end].to_string())
    }

    fn git(repository: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(repository)
            .output()
            .expect("run git command");
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("git stdout is utf8")
            .trim()
            .to_string()
    }

    fn assert_no_candidate_residue(repository: &Path) {
        let status = git(repository, &["status", "--short"]);
        assert!(
            status.is_empty() || status == "?? .jcode/",
            "unexpected canonical repository residue: {status}"
        );
        assert!(
            !repository.join("candidate-output.txt").exists(),
            "candidate output must remain isolated until promotion"
        );
    }

    fn install_pi_schema(path: &Path) {
        let connection = Connection::open(path).expect("open temp pi db");
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
            .expect("install temp pi schema");
    }

    fn ballot(
        round_id: jcode_tasker_types::AdjudicationRoundId,
        validator: &str,
        candidate_id: CandidateId,
    ) -> AdjudicationBallot {
        AdjudicationBallot {
            id: BallotId::new(),
            round_id,
            validator: ValidatorIdentity {
                session_id: format!("session-{validator}"),
                agent_id: validator.to_string(),
                model_id: Some("validator-model".to_string()),
                lineage_digest: format!("lineage-{validator}"),
            },
            assessments: vec![CandidateAssessment {
                candidate_id,
                eligible: true,
                approve: true,
                acceptance_score: 90,
                risk_score: 10,
                complexity_score: 10,
                notes: vec![format!("{validator} approved")],
            }],
            ranking: vec![candidate_id],
            abstained: false,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn real_headless_candidate_host_rejects_exclusive_policy_without_spawning_children() {
        let fixture = DogfoodFixture::new(1).await;
        let error = fixture
            .host
            .execute_request(
                fixture.request("exclusive-policy", json!({"kind": "exclusive"}), 1),
                CancellationToken::new(),
            )
            .await
            .expect_err("exclusive candidate lane launch must be rejected");

        assert!(
            error
                .to_string()
                .contains("exclusive policy cannot provision parallel candidate lanes"),
            "unexpected error: {error:#}"
        );
        assert_eq!(fixture.provider_state.calls.load(Ordering::SeqCst), 0);
        assert_eq!(
            fixture
                .sessions
                .read()
                .await
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![fixture.origin_session_id.clone()],
            "rejected exclusive launches must not leave headless children registered"
        );
        assert_eq!(
            git(&fixture.repository, &["rev-parse", CANONICAL_REF]),
            fixture.base_commit,
            "canonical ref must not move on rejected exclusive launch"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn real_headless_speculative_lanes_overlap_and_record_host_derived_evidence() {
        let fixture = DogfoodFixture::new(2).await;
        let response = fixture
            .host
            .execute_request(
                fixture.request(
                    "speculative-headless",
                    json!({"kind": "speculative", "max_candidates": 2}),
                    2,
                ),
                CancellationToken::new(),
            )
            .await
            .expect("execute speculative headless lanes");

        assert_eq!(response.status, "completed");
        assert!(response.redacted);
        assert_eq!(fixture.provider_state.calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            fixture.provider_state.max_active.load(Ordering::SeqCst),
            2,
            "provider barrier proves both real headless child turns overlapped"
        );
        let outcomes = response.report["outcomes"]
            .as_array()
            .expect("report outcomes");
        assert_eq!(outcomes.len(), 2);
        for outcome in outcomes {
            assert_eq!(outcome["state"], "submitted");
            let submission = &outcome["submission"];
            assert!(submission["resultCommit"].as_str().is_some());
            assert!(
                submission["diffDigest"]
                    .as_str()
                    .is_some_and(|value| value.starts_with("sha256:"))
            );
            let evidence = submission["evidence"].as_array().expect("host evidence");
            assert_eq!(evidence[0]["kind"], "git_candidate");
            assert_eq!(evidence[0]["verified"], true);
            assert_eq!(evidence[0]["baseCommit"], fixture.base_commit);
            assert_eq!(evidence[0]["changeKind"], "committed");
            assert_eq!(evidence[0]["changedPaths"], json!(["candidate-output.txt"]));
            assert_eq!(evidence[1]["kind"], "acceptance_validation");
            assert_eq!(evidence[1]["status"], "passed");
        }
        assert!(
            !serde_json::to_string(&response.report)
                .expect("serialize report")
                .contains("untrusted-provider-claim"),
            "provider advisory evidence must not cross the host evidence boundary"
        );
        assert_eq!(
            fixture
                .sessions
                .read()
                .await
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![fixture.origin_session_id.clone()],
            "successful child sessions must be removed from the host registry"
        );
        assert_eq!(
            git(&fixture.repository, &["rev-parse", CANONICAL_REF]),
            fixture.base_commit,
            "submitted speculative lanes must not mutate the canonical ref"
        );
        assert_no_candidate_residue(&fixture.repository);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 3)]
    async fn real_headless_ensemble_adjudicates_promotes_with_cas_and_cleans_losers() {
        let fixture = DogfoodFixture::new(3).await;
        let response = fixture
            .host
            .execute_request(
                fixture.request(
                    "ensemble-headless",
                    json!({"kind": "ensemble", "candidate_count": 3, "quorum": 2}),
                    3,
                ),
                CancellationToken::new(),
            )
            .await
            .expect("execute ensemble headless lanes");
        assert_eq!(response.status, "completed");
        assert_eq!(fixture.provider_state.calls.load(Ordering::SeqCst), 3);
        assert_eq!(fixture.provider_state.max_active.load(Ordering::SeqCst), 3);

        let candidate_set_id: jcode_tasker_types::CandidateSetId =
            response.report["candidateSetId"]
                .as_str()
                .expect("candidate set id")
                .parse()
                .expect("parse candidate set id");
        let revision = response.report["revision"].as_u64().expect("revision");
        let outcomes = response.report["outcomes"].as_array().expect("outcomes");
        let candidate_ids = outcomes
            .iter()
            .map(|outcome| {
                outcome["candidateId"]
                    .as_str()
                    .expect("candidate id")
                    .parse::<CandidateId>()
                    .expect("parse candidate id")
            })
            .collect::<Vec<_>>();
        let commits = outcomes
            .iter()
            .map(|outcome| {
                outcome["submission"]["resultCommit"]
                    .as_str()
                    .expect("result commit")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(commits.iter().collect::<HashSet<_>>().len(), 3);
        let eligible_revision = fixture.mark_candidates_eligible(&candidate_ids, revision);
        assert_no_candidate_residue(&fixture.repository);

        let store = jcode_tasker_pi::ConcurrencyStore::open_path(
            &fixture.database,
            fixture.repository.to_string_lossy().into_owned(),
        )
        .expect("open isolated concurrency store");
        let git_adapter = jcode_tasker_git::GitCandidateAdapter::try_new(&fixture.repository)
            .expect("open isolated git adapter");
        let mut orchestrator =
            jcode_tasker_orchestration::CandidateOrchestrator::new(store, git_adapter)
                .expect("open orchestrator");
        assert!(
            orchestrator
                .git()
                .assert_isolated()
                .expect("candidate refs are isolated")
                .is_isolated()
        );
        let handoff = orchestrator
            .handoff_to_round(candidate_set_id, eligible_revision)
            .expect("handoff submitted ensemble to adjudication");
        let mut round = handoff.round;
        let first = round
            .submit_ballot(
                ballot(handoff.opened.round_id, "validator-one", candidate_ids[0]),
                handoff.opened.revision,
            )
            .expect("first ballot");
        assert_eq!(first.completion, RoundCompletion::Pending);
        let selected = round
            .submit_ballot(
                ballot(handoff.opened.round_id, "validator-two", candidate_ids[0]),
                first.revision,
            )
            .expect("quorum ballot");
        assert_eq!(selected.completion, RoundCompletion::QuorumReached);
        assert_eq!(selected.selected_candidate_id(), Some(candidate_ids[0]));
        drop(round);
        drop(orchestrator);

        let store = jcode_tasker_pi::ConcurrencyStore::open_path(
            &fixture.database,
            fixture.repository.to_string_lossy().into_owned(),
        )
        .expect("open promotion store");
        let git_adapter = jcode_tasker_git::GitCandidateAdapter::try_new(&fixture.repository)
            .expect("open promotion git adapter");
        let mut reconciler = jcode_tasker_promotion::PromotionReconciler::new(store, git_adapter);
        let mut stale_request = PromotionRequest::promote(
            fixture.project_id.to_string(),
            fixture.task_id.to_string(),
            candidate_set_id.to_string(),
            candidate_ids[0].to_string(),
            CANONICAL_REF,
            selected.revision - 1,
        )
        .with_intent_id("headless-stale-promotion");
        let stale = reconciler
            .promote(&stale_request)
            .expect_err("stale promotion revision must fail CAS");
        assert!(
            matches!(
                stale,
                jcode_tasker_promotion::PromotionSagaError::StaleTaskerRevision {
                    expected,
                    actual
                } if expected == selected.revision - 1 && actual == selected.revision
            ),
            "stale promotion must fail at the Tasker revision CAS boundary"
        );
        stale_request.intent_id = Some("unused-after-stale".to_string());
        assert_eq!(
            git(&fixture.repository, &["rev-parse", CANONICAL_REF]),
            fixture.base_commit,
            "failed CAS must leave canonical ref untouched"
        );

        let request = PromotionRequest::promote(
            fixture.project_id.to_string(),
            fixture.task_id.to_string(),
            candidate_set_id.to_string(),
            candidate_ids[0].to_string(),
            CANONICAL_REF,
            selected.revision,
        )
        .with_intent_id("headless-promote-winner");
        let receipt = reconciler
            .promote(&request)
            .expect("promote selected winner");
        assert_eq!(receipt.target_commit, commits[0]);
        assert_eq!(
            git(&fixture.repository, &["rev-parse", CANONICAL_REF]),
            commits[0]
        );
        assert_eq!(
            reconciler
                .git()
                .list_candidate_ref_names()
                .expect("candidate refs after promotion"),
            vec![
                jcode_tasker_git::CandidateRef::new(candidate_set_id, candidate_ids[0]).to_string()
            ],
            "promotion must clean loser refs and retain only the winner evidence ref"
        );
        for loser in candidate_ids.iter().skip(1) {
            assert_ne!(
                reconciler
                    .store()
                    .candidate(&loser.to_string())
                    .expect("read loser")
                    .expect("loser exists")["state"],
                "promoted"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn real_headless_candidate_cancellation_abandons_and_cleans_child_session() {
        let fixture = DogfoodFixture::hanging().await;
        let cancellation = CancellationToken::new();
        let host = fixture.host.clone();
        let request = fixture.request(
            "cancel-headless",
            json!({"kind": "speculative", "max_candidates": 1}),
            1,
        );
        let task_cancellation = cancellation.clone();
        let task =
            tokio::spawn(async move { host.execute_request(request, task_cancellation).await });

        wait_for_provider_calls(&fixture.provider_state, 1).await;
        cancellation.cancel();
        let error = task
            .await
            .expect("join cancelled candidate execution")
            .expect_err("cancelled candidate execution must fail");
        assert!(
            error.to_string().contains("was cancelled"),
            "unexpected cancellation error: {error:#}"
        );

        assert_eq!(
            fixture
                .sessions
                .read()
                .await
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            vec![fixture.origin_session_id.clone()],
            "acknowledged headless cancellation must remove the child from the host registry"
        );
        assert_eq!(
            git(&fixture.repository, &["rev-parse", CANONICAL_REF]),
            fixture.base_commit,
            "cancelled candidate execution must not move the canonical ref"
        );
        assert_no_candidate_residue(&fixture.repository);

        let store = jcode_tasker_pi::ConcurrencyStore::open_path(
            &fixture.database,
            fixture.repository.to_string_lossy().into_owned(),
        )
        .expect("open isolated concurrency store after cancellation");
        let git_adapter = jcode_tasker_git::GitCandidateAdapter::try_new(&fixture.repository)
            .expect("open isolated git adapter after cancellation");
        let orchestrator =
            jcode_tasker_orchestration::CandidateOrchestrator::new(store, git_adapter)
                .expect("open orchestrator after cancellation");
        let inventory = orchestrator
            .recovery_inventory(10)
            .expect("read recovery inventory");
        assert_eq!(inventory.lanes.len(), 1);
        assert!(inventory.lanes[0].worktree.path.is_dir());
        let cleanup_candidate_id = inventory.lanes[0].candidate_id;
        drop(orchestrator);

        let restarted_store = jcode_tasker_pi::ConcurrencyStore::open_path(
            &fixture.database,
            fixture.repository.to_string_lossy().into_owned(),
        )
        .expect("reopen isolated concurrency store for recovery");
        let restarted_git = jcode_tasker_git::GitCandidateAdapter::try_new(&fixture.repository)
            .expect("reopen isolated git adapter for recovery");
        let restarted =
            jcode_tasker_orchestration::CandidateOrchestrator::new(restarted_store, restarted_git)
                .expect("reopen orchestrator for recovery");
        restarted
            .cleanup_lane(cleanup_candidate_id)
            .expect("recover cancelled lane cleanup");
        assert!(
            restarted
                .recovery_inventory(10)
                .expect("read recovery inventory after cleanup")
                .lanes
                .is_empty(),
            "recovery cleanup should clear cancellation cleanup debt"
        );
        assert!(
            restarted
                .git()
                .list_candidate_ref_names()
                .expect("candidate refs after cancellation recovery")
                .is_empty(),
            "recovery cleanup should remove cancelled candidate refs"
        );
    }

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
        assert!(register_cancellation(
            "candidate-op-authority",
            "session-a",
            &cancellation
        ));
        assert!(!cancel_operation("session-b", "candidate-op-authority"));
        assert!(!cancellation.is_cancelled());
        assert!(cancel_operation("session-a", "candidate-op-authority"));
        assert!(cancellation.is_cancelled());
        unregister_cancellation("candidate-op-authority");
    }
}
