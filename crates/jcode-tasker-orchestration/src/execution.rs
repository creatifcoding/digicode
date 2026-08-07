use async_trait::async_trait;
use futures::{StreamExt, stream::FuturesUnordered};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::process::Stdio;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use super::{
    AcceptanceContract, CandidateLaneSpec, CandidateOrchestrator, CandidateSetId,
    CandidateSubmission, ConcurrencyPolicy, LaneState, OpenCandidateSetRequest, OrchestrationError,
    ProjectId, ProvenanceTemplate, TaskId, ValidationCommand,
};
use crate::candidate_set_project;

/// Hard host-side upper bound for one candidate set.
pub const MAX_CANDIDATE_LANES: u16 = 8;
/// Default wall-clock budget for one candidate model turn.
pub const DEFAULT_CANDIDATE_LANE_TIMEOUT_MS: u64 = 15 * 60 * 1_000;
/// Hard host-side upper bound for one candidate model turn.
pub const MAX_CANDIDATE_LANE_TIMEOUT_MS: u64 = 30 * 60 * 1_000;

fn default_max_lanes() -> u16 {
    MAX_CANDIDATE_LANES
}

fn default_lane_timeout_ms() -> u64 {
    DEFAULT_CANDIDATE_LANE_TIMEOUT_MS
}

const CANDIDATE_TERMINATION_TIMEOUT_MS: u64 = 11_000;
const CANDIDATE_VALIDATION_TIMEOUT_MS: u64 = 60_000;
const MAX_VALIDATION_OUTPUT_BYTES: usize = 64 * 1024;

/// Host-enforced execution limits. Guest values can only tighten these bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateLaneExecutionLimits {
    #[serde(default = "default_max_lanes")]
    pub max_lanes: u16,
    #[serde(default = "default_lane_timeout_ms")]
    pub lane_timeout_ms: u64,
}

impl Default for CandidateLaneExecutionLimits {
    fn default() -> Self {
        Self {
            max_lanes: MAX_CANDIDATE_LANES,
            lane_timeout_ms: DEFAULT_CANDIDATE_LANE_TIMEOUT_MS,
        }
    }
}

impl CandidateLaneExecutionLimits {
    pub fn validate(self) -> Result<Self, OrchestrationError> {
        if self.max_lanes == 0 || self.max_lanes > MAX_CANDIDATE_LANES {
            return Err(OrchestrationError::CandidateLimit {
                requested: self.max_lanes,
                limit: MAX_CANDIDATE_LANES,
            });
        }
        if self.lane_timeout_ms == 0 || self.lane_timeout_ms > MAX_CANDIDATE_LANE_TIMEOUT_MS {
            return Err(OrchestrationError::InvalidContract(format!(
                "lane timeout must be between 1ms and {MAX_CANDIDATE_LANE_TIMEOUT_MS}ms"
            )));
        }
        Ok(self)
    }

    pub fn timeout(self) -> Duration {
        Duration::from_millis(self.lane_timeout_ms)
    }
}

/// Declarative guest request admitted by the host candidate-lane runtime.
///
/// Authority-bearing identity, revision, provenance, and base-commit fields are
/// deliberately absent. A server-owned coordinator resolves those values from
/// its bound Tasker/session context before constructing a bound request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateLaneLaunchRequest {
    /// Guest-proposed task reference. The host resolves it against canonical
    /// Tasker state and never treats it as an authority-bearing task id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_reference: Option<String>,
    pub policy: ConcurrencyPolicy,
    pub lane_count: u16,
    pub acceptance: AcceptanceContract,
    pub prompt: String,
    #[serde(default)]
    pub limits: CandidateLaneExecutionLimits,
}

impl CandidateLaneLaunchRequest {
    pub fn validate(&self) -> Result<CandidateLaneExecutionLimits, OrchestrationError> {
        if self.prompt.trim().is_empty() {
            return Err(OrchestrationError::InvalidContract(
                "candidate prompt must not be empty".into(),
            ));
        }
        let limits = self.limits.validate()?;
        if self.lane_count == 0 || self.lane_count > limits.max_lanes {
            return Err(OrchestrationError::CandidateLimit {
                requested: self.lane_count,
                limit: limits.max_lanes,
            });
        }
        self.acceptance.validate()?;
        self.policy.validate().map_err(OrchestrationError::Policy)?;
        if matches!(self.policy, ConcurrencyPolicy::Exclusive) {
            return Err(OrchestrationError::ExclusivePolicy);
        }
        Ok(limits)
    }

    /// Bind guest intent to values resolved and validated by the host.
    pub fn bind(
        self,
        host: CandidateLaneHostContext,
    ) -> Result<BoundCandidateLaneLaunchRequest, OrchestrationError> {
        self.validate()?;
        host.validate()?;
        let mut guest = self;
        guest.acceptance.validation_commands = host.validation_commands.clone();
        Ok(BoundCandidateLaneLaunchRequest { guest, host })
    }
}

/// Host-resolved Tasker/session authority for one candidate-lane execution.
///
/// This type is intentionally not deserializable. Callers must resolve these
/// values from the bound Tasker context rather than forwarding guest claims.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateLaneHostContext {
    pub project_id: ProjectId,
    pub task_id: TaskId,
    pub base_commit: String,
    pub provenance: ProvenanceTemplate,
    pub expected_revision: u64,
    /// Validators are host-owned. Guest-proposed validators are replaced by
    /// this validated, allowlisted set before the candidate set is opened.
    pub validation_commands: Vec<ValidationCommand>,
}

impl CandidateLaneHostContext {
    pub fn validate(&self) -> Result<(), OrchestrationError> {
        if self.base_commit.trim().is_empty() {
            return Err(OrchestrationError::InvalidContract(
                "host-resolved candidate base_commit must not be empty".into(),
            ));
        }
        if self.provenance.session_id.trim().is_empty()
            || self.provenance.agent_id.trim().is_empty()
        {
            return Err(OrchestrationError::InvalidContract(
                "host-resolved candidate provenance must identify the session and agent".into(),
            ));
        }
        if self.validation_commands.is_empty() {
            return Err(OrchestrationError::InvalidContract(
                "host-resolved candidate validators must not be empty".into(),
            ));
        }
        for command in &self.validation_commands {
            command.validate()?;
        }
        Ok(())
    }
}

/// A guest-safe lane request after the host has attached Tasker authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundCandidateLaneLaunchRequest {
    guest: CandidateLaneLaunchRequest,
    host: CandidateLaneHostContext,
}

impl BoundCandidateLaneLaunchRequest {
    pub fn validate(&self) -> Result<CandidateLaneExecutionLimits, OrchestrationError> {
        self.guest.validate()?;
        self.host.validate()?;
        Ok(self.guest.limits)
    }

    pub fn into_open_request(
        &self,
        worktree_root: impl Into<std::path::PathBuf>,
    ) -> OpenCandidateSetRequest {
        let mut request = OpenCandidateSetRequest::new(
            self.host.project_id,
            self.host.task_id,
            self.host.base_commit.clone(),
            self.guest.policy.clone(),
            self.guest.lane_count,
            self.guest.acceptance.clone(),
            self.host.provenance.clone(),
            worktree_root,
        );
        request.expected_revision = Some(self.host.expected_revision);
        request
    }

    fn prompt(&self) -> &str {
        &self.guest.prompt
    }
}

/// The only request a host candidate executor receives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateLaneExecutionRequest {
    pub candidate_set_id: CandidateSetId,
    pub lane: CandidateLaneSpec,
    pub prompt: String,
    pub timeout: Duration,
}

/// Errors returned by a candidate executor before the host records lane state.
#[derive(Debug, Error)]
pub enum CandidateExecutorError {
    #[error("candidate executor failed: {0}")]
    Failed(String),
    #[error("candidate executor was cancelled")]
    Cancelled,
}

/// Host-owned execution boundary. Implementations may call a provider, but
/// they never receive a Tasker database connection or a canonical ref writer.
#[async_trait]
pub trait CandidateExecutor: Send + Sync {
    async fn execute(
        &self,
        request: CandidateLaneExecutionRequest,
        cancellation: CancellationToken,
    ) -> Result<CandidateAgentResult, CandidateExecutorError>;

    /// Signal the provider/session adapter to stop the in-flight turn. The
    /// default is safe for executors whose `execute` future directly observes
    /// the supplied cancellation token.
    async fn cancel(
        &self,
        _request: &CandidateLaneExecutionRequest,
    ) -> Result<(), CandidateExecutorError> {
        Ok(())
    }
}

/// Untrusted output from a candidate agent. Git identity, digests, and
/// evidence are deliberately absent because the host derives them after the
/// detached worktree and acceptance validators have been checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateAgentResult {
    pub summary: Option<String>,
    #[serde(default)]
    pub advisory_evidence: Vec<Value>,
}

impl CandidateAgentResult {
    pub fn new(summary: impl Into<String>) -> Self {
        Self {
            summary: Some(summary.into()),
            advisory_evidence: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateLaneFailure {
    pub candidate_id: super::CandidateId,
    pub state: LaneState,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateLaneOutcome {
    pub candidate_id: super::CandidateId,
    pub state: LaneState,
    pub submission: Option<CandidateSubmission>,
    pub failure: Option<CandidateLaneFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateExecutionReport {
    pub candidate_set_id: CandidateSetId,
    pub revision: u64,
    pub outcomes: Vec<CandidateLaneOutcome>,
}

impl CandidateExecutionReport {
    pub fn submitted_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.state == LaneState::Submitted)
            .count()
    }

    pub fn as_value(&self) -> Value {
        serde_json::to_value(self).expect("candidate execution report is serializable")
    }
}

fn candidate_prompt(prompt: &str, lane: &CandidateLaneSpec) -> Result<String, OrchestrationError> {
    let acceptance = serde_json::to_string_pretty(&lane.acceptance)?;
    Ok(format!(
        "{prompt}\n\nTasker candidate lane contract:\n- Work only in `{}`.\n- Run headless or inline only. Do not spawn agents or open visible terminals.\n- Do not modify canonical refs.\n- Candidate ref: `{}`.\n- Candidate id: `{}`.\n- Run every validation command in the acceptance contract.\n- Commit all changes before returning.\n- Return one JSON object with `summary` after the work is complete. The host derives the commit, diff digest, and evidence.\n\nAcceptance contract:\n{acceptance}",
        lane.worktree.path.display(),
        lane.candidate_ref,
        lane.candidate_id,
    ))
}

fn validate_agent_result(result: &CandidateAgentResult) -> Result<(), OrchestrationError> {
    if result
        .summary
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        return Err(OrchestrationError::InvalidContract(
            "candidate agent result requires a summary".into(),
        ));
    }
    Ok(())
}

struct ValidationOutput {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

enum ValidationRunError {
    Cancelled,
    Failed(String),
}

fn bounded_output(bytes: &[u8]) -> String {
    let mut value = String::from_utf8_lossy(bytes).into_owned();
    if value.len() > MAX_VALIDATION_OUTPUT_BYTES {
        value.truncate(MAX_VALIDATION_OUTPUT_BYTES);
        value.push_str("\n[output truncated by host]");
    }
    value
}

async fn run_validation_command(
    worktree_path: &std::path::Path,
    command: &ValidationCommand,
    cancellation: &CancellationToken,
) -> Result<ValidationOutput, ValidationRunError> {
    let mut child = TokioCommand::new(&command.program);
    child
        .args(&command.args)
        .current_dir(worktree_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::select! {
        _ = cancellation.cancelled() => return Err(ValidationRunError::Cancelled),
        result = timeout(Duration::from_millis(CANDIDATE_VALIDATION_TIMEOUT_MS), child.output()) => {
            match result {
                Ok(Ok(output)) => output,
                Ok(Err(error)) => return Err(ValidationRunError::Failed(format!(
                    "validation command {} failed to start: {error}",
                    command.program
                ))),
                Err(_) => return Err(ValidationRunError::Failed(format!(
                    "validation command {} exceeded {CANDIDATE_VALIDATION_TIMEOUT_MS}ms",
                    command.program
                ))),
            }
        }
    };
    let stdout = bounded_output(&output.stdout);
    let stderr = bounded_output(&output.stderr);
    if !output.status.success() {
        return Err(ValidationRunError::Failed(format!(
            "validation command {} exited with {}",
            command.program, output.status
        )));
    }
    Ok(ValidationOutput {
        exit_code: output.status.code(),
        stdout,
        stderr,
    })
}

fn validation_output_digest(output: &ValidationOutput) -> String {
    let mut bytes = Vec::with_capacity(output.stdout.len() + output.stderr.len() + 1);
    bytes.extend_from_slice(output.stdout.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(output.stderr.as_bytes());
    format!("sha256:{:x}", Sha256::digest(bytes))
}

enum LaneRunResult {
    Submitted(CandidateAgentResult),
    Failed(String),
    Cancelled,
    TimedOut,
    TerminationFailed(String),
}

async fn run_lane<E: CandidateExecutor>(
    executor: &E,
    execution: CandidateLaneExecutionRequest,
    cancellation: CancellationToken,
) -> LaneRunResult {
    let lane_cancellation = cancellation.child_token();
    let mut execution_future =
        std::pin::pin!(executor.execute(execution.clone(), lane_cancellation.clone(),));
    tokio::select! {
        _ = cancellation.cancelled() => {
            lane_cancellation.cancel();
            if let Err(error) = executor.cancel(&execution).await {
                return LaneRunResult::TerminationFailed(format!(
                    "candidate executor rejected cancellation: {error}"
                ));
            }
            match timeout(
                Duration::from_millis(CANDIDATE_TERMINATION_TIMEOUT_MS),
                &mut execution_future,
            )
            .await
            {
                Ok(Ok(_)) | Ok(Err(CandidateExecutorError::Cancelled)) => LaneRunResult::Cancelled,
                Ok(Err(CandidateExecutorError::Failed(reason))) => {
                    LaneRunResult::TerminationFailed(reason)
                }
                Err(_) => LaneRunResult::TerminationFailed(
                    "candidate executor did not acknowledge cancellation before the termination deadline"
                        .into(),
                ),
            }
        }
        result = timeout(execution.timeout, &mut execution_future) => {
            match result {
                Ok(Ok(result)) => LaneRunResult::Submitted(result),
                Ok(Err(CandidateExecutorError::Cancelled)) => LaneRunResult::Cancelled,
                Ok(Err(CandidateExecutorError::Failed(reason))) => LaneRunResult::Failed(reason),
                Err(_) => {
                    lane_cancellation.cancel();
                    if let Err(error) = executor.cancel(&execution).await {
                        return LaneRunResult::TerminationFailed(format!(
                            "candidate executor rejected timeout termination: {error}"
                        ));
                    }
                    match timeout(
                        Duration::from_millis(CANDIDATE_TERMINATION_TIMEOUT_MS),
                        &mut execution_future,
                    )
                    .await
                    {
                        Ok(Ok(_)) | Ok(Err(CandidateExecutorError::Cancelled)) => LaneRunResult::TimedOut,
                        Ok(Err(CandidateExecutorError::Failed(reason))) => {
                            LaneRunResult::TerminationFailed(reason)
                        }
                        Err(_) => LaneRunResult::TerminationFailed(
                            "candidate executor did not acknowledge timeout termination before the termination deadline"
                                .into(),
                        ),
                    }
                }
            }
        }
    }
}

impl CandidateOrchestrator {
    /// Execute a candidate set through a host-owned executor.
    ///
    /// Lane persistence remains synchronous and CAS-protected. Model work is
    /// fanned out within the host lane bound, then terminal results are
    /// reconciled in stable lane order so SQLite revisions cannot race. A
    /// failed, cancelled, or timed-out lane is terminally recorded and cleanup
    /// is attempted before the report is returned. Unacknowledged termination
    /// produces a quarantined outcome and retains its Git resources for
    /// recovery instead of returning early.
    pub async fn execute_candidate_set<E: CandidateExecutor>(
        &mut self,
        request: BoundCandidateLaneLaunchRequest,
        worktree_root: impl Into<std::path::PathBuf>,
        executor: &E,
        cancellation: CancellationToken,
    ) -> Result<CandidateExecutionReport, OrchestrationError> {
        let limits = request.validate()?;
        let opened = self.open_candidate_set(request.clone().into_open_request(worktree_root))?;
        let mut revision = opened.revision;
        if cancellation.is_cancelled() {
            let revision = self.abandon_remaining_lanes(&opened.lanes, revision)?;
            return Err(OrchestrationError::ExecutionCancelled {
                candidate_set_id: opened.candidate_set.id.to_string(),
                revision,
            });
        }

        // Claim every lane before fanning out. This makes the host's CAS
        // lifecycle explicit and ensures cancellation can clean every lane.
        for lane in &opened.lanes {
            revision = self.start_lane(lane.candidate_id, revision)?.revision;
        }

        let mut jobs = FuturesUnordered::new();
        for (index, lane) in opened.lanes.iter().cloned().enumerate() {
            let prompt = match candidate_prompt(request.prompt(), &lane) {
                Ok(prompt) => prompt,
                Err(error) => {
                    self.abandon_remaining_lanes(&opened.lanes, revision)?;
                    return Err(error);
                }
            };
            let execution = CandidateLaneExecutionRequest {
                candidate_set_id: opened.candidate_set.id,
                lane: lane.clone(),
                prompt,
                timeout: limits.timeout(),
            };
            let cancellation = cancellation.clone();
            jobs.push(async move {
                let result = tokio::select! {
                    result = run_lane(executor, execution, cancellation.clone()) => result
                };
                (index, lane, result)
            });
        }

        let mut completed = Vec::with_capacity(opened.lanes.len());
        while let Some(result) = jobs.next().await {
            completed.push(result);
        }
        completed.sort_by_key(|(index, _, _)| *index);

        let mut outcomes = Vec::with_capacity(completed.len());
        let mut cancelled = false;
        for (_, lane, result) in completed {
            let outcome = match result {
                LaneRunResult::Submitted(submission) => {
                    match self
                        .host_validate_submission(&lane, submission, &cancellation)
                        .await
                    {
                        Ok(submission) => {
                            match self.submit_lane(lane.candidate_id, revision, submission.clone())
                            {
                                Ok(receipt) => {
                                    revision = receipt.revision;
                                    CandidateLaneOutcome {
                                        candidate_id: lane.candidate_id,
                                        state: LaneState::Submitted,
                                        submission: Some(submission),
                                        failure: None,
                                    }
                                }
                                Err(error) => {
                                    let reason = error.to_string();
                                    let (outcome, next_revision) =
                                        self.failed_outcome(&lane, revision, reason)?;
                                    revision = next_revision;
                                    outcome
                                }
                            }
                        }
                        Err(error) => {
                            let (outcome, next_revision) = if matches!(
                                &error,
                                OrchestrationError::ExecutionCancelled { .. }
                            ) {
                                cancelled = true;
                                self.abandoned_outcome(
                                    &lane,
                                    revision,
                                    "candidate execution cancelled during acceptance validation",
                                )?
                            } else {
                                self.failed_outcome(&lane, revision, error.to_string())?
                            };
                            revision = next_revision;
                            outcome
                        }
                    }
                }
                LaneRunResult::Failed(reason) => {
                    let (outcome, next_revision) = self.failed_outcome(&lane, revision, reason)?;
                    revision = next_revision;
                    outcome
                }
                LaneRunResult::Cancelled => {
                    cancelled = true;
                    let (outcome, next_revision) =
                        self.abandoned_outcome(&lane, revision, "candidate execution cancelled")?;
                    revision = next_revision;
                    outcome
                }
                LaneRunResult::TimedOut => {
                    let reason = format!(
                        "candidate lane exceeded {}ms timeout",
                        limits.lane_timeout_ms
                    );
                    let (outcome, next_revision) =
                        self.timed_out_outcome(&lane, revision, reason)?;
                    revision = next_revision;
                    outcome
                }
                LaneRunResult::TerminationFailed(reason) => {
                    let transition =
                        self.quarantine_lane(lane.candidate_id, revision, reason.clone())?;
                    revision = transition.revision;
                    CandidateLaneOutcome {
                        candidate_id: lane.candidate_id,
                        state: LaneState::Quarantined,
                        submission: None,
                        failure: Some(CandidateLaneFailure {
                            candidate_id: lane.candidate_id,
                            state: LaneState::Quarantined,
                            reason,
                        }),
                    }
                }
            };
            outcomes.push(outcome);
        }

        if cancelled || cancellation.is_cancelled() {
            return Err(OrchestrationError::ExecutionCancelled {
                candidate_set_id: opened.candidate_set.id.to_string(),
                revision,
            });
        }

        Ok(CandidateExecutionReport {
            candidate_set_id: opened.candidate_set.id,
            revision,
            outcomes,
        })
    }

    fn failed_outcome(
        &mut self,
        lane: &CandidateLaneSpec,
        revision: u64,
        reason: String,
    ) -> Result<(CandidateLaneOutcome, u64), OrchestrationError> {
        self.terminal_cleanup_outcome(lane, revision, LaneState::Failed, reason)
    }

    fn abandoned_outcome(
        &mut self,
        lane: &CandidateLaneSpec,
        revision: u64,
        reason: &str,
    ) -> Result<(CandidateLaneOutcome, u64), OrchestrationError> {
        self.terminal_cleanup_outcome(lane, revision, LaneState::Abandoned, reason.to_owned())
    }

    fn timed_out_outcome(
        &mut self,
        lane: &CandidateLaneSpec,
        revision: u64,
        reason: String,
    ) -> Result<(CandidateLaneOutcome, u64), OrchestrationError> {
        self.terminal_cleanup_outcome(lane, revision, LaneState::TimedOut, reason)
    }

    fn terminal_cleanup_outcome(
        &mut self,
        lane: &CandidateLaneSpec,
        revision: u64,
        state: LaneState,
        reason: String,
    ) -> Result<(CandidateLaneOutcome, u64), OrchestrationError> {
        debug_assert!(matches!(
            state,
            LaneState::Failed | LaneState::Abandoned | LaneState::TimedOut
        ));
        let transition =
            self.transition(lane.candidate_id, revision, state, Some(reason.clone()))?;
        match self.cleanup_lane(lane.candidate_id) {
            Ok(_) => Ok((
                CandidateLaneOutcome {
                    candidate_id: lane.candidate_id,
                    state,
                    submission: None,
                    failure: Some(CandidateLaneFailure {
                        candidate_id: lane.candidate_id,
                        state,
                        reason,
                    }),
                },
                transition.revision,
            )),
            Err(cleanup_error) => {
                let reason = format!("{reason}; cleanup retained for recovery: {cleanup_error}");
                let quarantine =
                    self.quarantine_lane(lane.candidate_id, transition.revision, reason.clone())?;
                Ok((
                    CandidateLaneOutcome {
                        candidate_id: lane.candidate_id,
                        state: LaneState::Quarantined,
                        submission: None,
                        failure: Some(CandidateLaneFailure {
                            candidate_id: lane.candidate_id,
                            state: LaneState::Quarantined,
                            reason,
                        }),
                    },
                    quarantine.revision,
                ))
            }
        }
    }

    async fn host_validate_submission(
        &self,
        lane: &CandidateLaneSpec,
        agent_result: CandidateAgentResult,
        cancellation: &CancellationToken,
    ) -> Result<CandidateSubmission, OrchestrationError> {
        validate_agent_result(&agent_result)?;
        let candidate_ref =
            jcode_tasker_git::CandidateRef::parse(&lane.candidate_ref).map_err(super::git_error)?;
        let metadata = self
            .git
            .finalize_candidate_worktree(&candidate_ref, &lane.worktree.path)
            .map_err(super::git_error)?;

        let mut evidence = vec![json!({
            "ref": candidate_ref.to_string(),
            "kind": "git_candidate",
            "verified": true,
            "baseCommit": metadata.base_oid,
            "commit": metadata.tip_oid,
            "changeKind": if metadata.base_oid == metadata.tip_oid { "no_op" } else { "committed" },
            "changedPaths": metadata.changed_paths,
        })];
        for (index, command) in lane.acceptance.validation_commands.iter().enumerate() {
            if cancellation.is_cancelled() {
                return Err(OrchestrationError::ExecutionCancelled {
                    candidate_set_id: candidate_ref.candidate_set_id().to_string(),
                    revision: self.store.current_revision(&candidate_set_project(
                        &self.store,
                        candidate_ref.candidate_set_id(),
                    )?)?,
                });
            }
            let output =
                match run_validation_command(&lane.worktree.path, command, cancellation).await {
                    Ok(output) => output,
                    Err(ValidationRunError::Cancelled) => {
                        return Err(OrchestrationError::ExecutionCancelled {
                            candidate_set_id: candidate_ref.candidate_set_id().to_string(),
                            revision: self.store.current_revision(&candidate_set_project(
                                &self.store,
                                candidate_ref.candidate_set_id(),
                            )?)?,
                        });
                    }
                    Err(ValidationRunError::Failed(reason)) => {
                        return Err(OrchestrationError::InvalidContract(reason));
                    }
                };
            let reference = format!("validation://{}/{}", lane.candidate_id, index);
            evidence.push(json!({
                "ref": reference,
                "kind": "acceptance_validation",
                "program": command.program,
                "argCount": command.args.len(),
                "status": "passed",
                "exitCode": output.exit_code,
                "stdoutBytes": output.stdout.len(),
                "stderrBytes": output.stderr.len(),
                "outputDigest": validation_output_digest(&output),
            }));
        }
        self.git
            .ensure_worktree_clean(&lane.worktree.path)
            .map_err(super::git_error)?;
        let final_worktree_tip = self
            .git
            .read_worktree_head(&lane.worktree.path)
            .map_err(super::git_error)?;
        if final_worktree_tip != metadata.tip_oid {
            return Err(OrchestrationError::InvalidContract(
                "acceptance validation changed the detached candidate tip".into(),
            ));
        }
        let final_metadata = self
            .git
            .read_candidate_metadata(&candidate_ref)
            .map_err(super::git_error)?;
        if final_metadata.tip_oid != metadata.tip_oid {
            return Err(OrchestrationError::InvalidContract(
                "candidate ref changed during acceptance validation".into(),
            ));
        }
        let diff_digest = self
            .git
            .candidate_diff_digest(&candidate_ref)
            .map_err(super::git_error)?;
        if let Some(git_evidence) = evidence.first_mut() {
            git_evidence["diffDigest"] = json!(diff_digest.clone());
        }
        Ok(CandidateSubmission {
            result_commit: final_metadata.tip_oid,
            diff_digest: Some(diff_digest),
            summary: agent_result.summary,
            evidence,
        })
    }

    fn abandon_remaining_lanes(
        &mut self,
        lanes: &[CandidateLaneSpec],
        mut revision: u64,
    ) -> Result<u64, OrchestrationError> {
        let mut errors = Vec::new();
        for lane in lanes {
            match self.abandoned_outcome(lane, revision, "candidate execution cancelled") {
                Ok((_, next_revision)) => revision = next_revision,
                Err(error) => errors.push(format!("{}: {error}", lane.candidate_id)),
            }
        }
        if !errors.is_empty() {
            return Err(OrchestrationError::Git {
                message: format!(
                    "aggregate cancellation cleanup failed: {}",
                    errors.join("; ")
                ),
            });
        }
        Ok(revision)
    }
}

/// Stable declarative shape used by provider-facing tests and guides.
pub fn candidate_lane_effect_payload(request: &CandidateLaneLaunchRequest) -> Value {
    json!(request)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValidationCommand;
    use jcode_tasker_types::{ConcurrencyPolicy, ProjectId, TaskId};
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;
    use tokio::sync::{Barrier, Notify};

    #[derive(Clone)]
    struct FakeExecutor {
        calls: Arc<AtomicUsize>,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        submissions: Arc<Mutex<Vec<CandidateAgentResult>>>,
        barrier: Option<Arc<Barrier>>,
        fail_on: Option<usize>,
        commit: bool,
        delay: Option<Duration>,
    }

    impl FakeExecutor {
        fn record_max(&self, current: usize) {
            let mut observed = self.max_active.load(Ordering::SeqCst);
            while current > observed {
                match self.max_active.compare_exchange(
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

    #[derive(Clone)]
    struct CancellationAcknowledgingExecutor {
        started: Arc<Notify>,
        cancel_calls: Arc<AtomicUsize>,
        released: Arc<Notify>,
    }

    #[derive(Clone)]
    struct NonAcknowledgingExecutor {
        cancel_calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl CandidateExecutor for CancellationAcknowledgingExecutor {
        async fn execute(
            &self,
            _request: CandidateLaneExecutionRequest,
            _cancellation: CancellationToken,
        ) -> Result<CandidateAgentResult, CandidateExecutorError> {
            self.started.notify_one();
            self.released.notified().await;
            Err(CandidateExecutorError::Cancelled)
        }

        async fn cancel(
            &self,
            _request: &CandidateLaneExecutionRequest,
        ) -> Result<(), CandidateExecutorError> {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
            self.released.notify_one();
            Ok(())
        }
    }

    #[async_trait]
    impl CandidateExecutor for NonAcknowledgingExecutor {
        async fn execute(
            &self,
            _request: CandidateLaneExecutionRequest,
            _cancellation: CancellationToken,
        ) -> Result<CandidateAgentResult, CandidateExecutorError> {
            std::future::pending().await
        }

        async fn cancel(
            &self,
            _request: &CandidateLaneExecutionRequest,
        ) -> Result<(), CandidateExecutorError> {
            self.cancel_calls.fetch_add(1, Ordering::SeqCst);
            Err(CandidateExecutorError::Failed(
                "provider termination is unacknowledged".into(),
            ))
        }
    }

    #[async_trait]
    impl CandidateExecutor for FakeExecutor {
        async fn execute(
            &self,
            request: CandidateLaneExecutionRequest,
            cancellation: CancellationToken,
        ) -> Result<CandidateAgentResult, CandidateExecutorError> {
            let index = self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.record_max(active);
            if let Some(barrier) = &self.barrier {
                tokio::select! {
                    _ = barrier.wait() => {}
                    _ = cancellation.cancelled() => {
                        self.active.fetch_sub(1, Ordering::SeqCst);
                        return Err(CandidateExecutorError::Cancelled);
                    }
                }
            }
            if let Some(delay) = self.delay {
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = cancellation.cancelled() => {
                        self.active.fetch_sub(1, Ordering::SeqCst);
                        return Err(CandidateExecutorError::Cancelled);
                    }
                }
            }
            if cancellation.is_cancelled() {
                self.active.fetch_sub(1, Ordering::SeqCst);
                return Err(CandidateExecutorError::Cancelled);
            }
            if self.fail_on == Some(index) {
                self.active.fetch_sub(1, Ordering::SeqCst);
                return Err(CandidateExecutorError::Failed("fake failure".into()));
            }
            if self.commit {
                let file = request
                    .lane
                    .worktree
                    .path
                    .join(format!("candidate-{index}.txt"));
                std::fs::write(file, format!("candidate {index}\n"))
                    .map_err(|error| CandidateExecutorError::Failed(error.to_string()))?;
                for args in [
                    vec!["add".to_owned(), ".".to_owned()],
                    vec![
                        "commit".to_owned(),
                        "--quiet".to_owned(),
                        "-m".to_owned(),
                        "candidate".to_owned(),
                    ],
                ] {
                    let status = Command::new("git")
                        .args(args)
                        .current_dir(&request.lane.worktree.path)
                        .status()
                        .map_err(|error| CandidateExecutorError::Failed(error.to_string()))?;
                    if !status.success() {
                        self.active.fetch_sub(1, Ordering::SeqCst);
                        return Err(CandidateExecutorError::Failed(
                            "fake git command failed".into(),
                        ));
                    }
                }
            }
            let result = CandidateAgentResult {
                summary: Some(format!("fake candidate {index}")),
                advisory_evidence: vec![json!({"ref": "model://untrusted"})],
            };
            self.submissions
                .lock()
                .expect("fake lock")
                .push(result.clone());
            self.active.fetch_sub(1, Ordering::SeqCst);
            Ok(result)
        }
    }

    fn request_for(
        root: &TempDir,
        lane_count: u16,
    ) -> (CandidateOrchestrator, BoundCandidateLaneLaunchRequest) {
        let repo = root.path().join("repo");
        std::fs::create_dir_all(&repo).expect("create repo");
        for args in [
            vec!["init", "--quiet"],
            vec!["config", "user.name", "Tasker Test"],
            vec!["config", "user.email", "tasker@example.invalid"],
        ] {
            Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .expect("configure git");
        }
        std::fs::write(repo.join("base.txt"), "base\n").expect("write base");
        for args in [
            vec!["add", "base.txt"],
            vec!["commit", "--quiet", "-m", "base"],
        ] {
            Command::new("git")
                .args(args)
                .current_dir(&repo)
                .status()
                .expect("commit base");
        }
        let base_commit = String::from_utf8(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&repo)
                .output()
                .expect("rev parse")
                .stdout,
        )
        .expect("utf8")
        .trim()
        .to_owned();
        let db = root.path().join("tasker.sqlite");
        let project_id = ProjectId::new();
        let store = jcode_tasker_pi::ConcurrencyStore::open_path(&db, repo.to_string_lossy())
            .expect("store");
        let git = jcode_tasker_git::GitCandidateAdapter::try_new(&repo).expect("git adapter");
        let orchestrator = CandidateOrchestrator::new(store, git).expect("orchestrator");
        let request = CandidateLaneLaunchRequest {
            task_reference: None,
            policy: ConcurrencyPolicy::Speculative { max_candidates: 3 },
            lane_count,
            acceptance: AcceptanceContract::new(
                vec![ValidationCommand::new("true", std::iter::empty::<&str>())],
                "fake acceptance",
                Vec::new(),
            ),
            prompt: "Implement the task".into(),
            limits: CandidateLaneExecutionLimits {
                max_lanes: 3,
                lane_timeout_ms: 5_000,
            },
        }
        .bind(CandidateLaneHostContext {
            project_id,
            task_id: TaskId::new(),
            base_commit,
            provenance: ProvenanceTemplate::new("session", "agent"),
            expected_revision: 0,
            validation_commands: vec![ValidationCommand::new("true", std::iter::empty::<&str>())],
        })
        .expect("bind host candidate context");
        (orchestrator, request)
    }

    fn reopen_orchestrator(root: &TempDir) -> CandidateOrchestrator {
        let repo = root.path().join("repo");
        let db = root.path().join("tasker.sqlite");
        let store = jcode_tasker_pi::ConcurrencyStore::open_path(&db, repo.to_string_lossy())
            .expect("reopen store");
        let git = jcode_tasker_git::GitCandidateAdapter::try_new(&repo).expect("reopen git");
        CandidateOrchestrator::new(store, git).expect("reopen orchestrator")
    }

    fn fake_executor() -> FakeExecutor {
        FakeExecutor {
            calls: Arc::new(AtomicUsize::new(0)),
            active: Arc::new(AtomicUsize::new(0)),
            max_active: Arc::new(AtomicUsize::new(0)),
            submissions: Arc::new(Mutex::new(Vec::new())),
            barrier: None,
            fail_on: None,
            commit: true,
            delay: None,
        }
    }

    #[tokio::test]
    async fn lanes_overlap_and_reconcile_in_stable_order_with_host_evidence() {
        let root = TempDir::new().expect("temp root");
        let (mut orchestrator, request) = request_for(&root, 2);
        let mut executor = fake_executor();
        executor.barrier = Some(Arc::new(Barrier::new(2)));
        let report = orchestrator
            .execute_candidate_set(
                request,
                root.path().join("lanes"),
                &executor,
                CancellationToken::new(),
            )
            .await
            .expect("execute fake lanes");
        assert_eq!(report.submitted_count(), 2);
        assert_eq!(executor.max_active.load(Ordering::SeqCst), 2);
        assert!(
            report
                .outcomes
                .iter()
                .all(|outcome| outcome.state == LaneState::Submitted)
        );
        for outcome in report.outcomes {
            let submission = outcome.submission.expect("submission");
            assert!(
                submission
                    .diff_digest
                    .expect("digest")
                    .starts_with("sha256:")
            );
            assert_eq!(submission.evidence[0]["kind"], "git_candidate");
            assert_eq!(submission.evidence[0]["verified"], true);
            let validation = submission
                .evidence
                .iter()
                .find(|entry| entry["kind"] == "acceptance_validation")
                .expect("validation evidence");
            assert!(validation.get("stdout").is_none());
            assert!(validation.get("stderr").is_none());
            assert!(validation["outputDigest"].as_str().is_some());
        }
        let status = orchestrator
            .status(report.candidate_set_id, 8)
            .expect("status");
        assert_eq!(status.counts.submitted, 2);
        assert_eq!(status.counts.in_progress, 0);
        assert_eq!(status.revision, report.revision);
        assert!(
            orchestrator
                .git()
                .list_candidate_ref_names()
                .expect("refs")
                .len()
                == 2
        );
    }

    #[test]
    fn guest_request_rejects_authority_bearing_fields() {
        let request = json!({
            "policy": {"kind": "speculative", "max_candidates": 2},
            "laneCount": 2,
            "acceptance": {
                "validationCommands": [{"program": "true", "args": []}],
                "acceptanceCriteria": "test",
                "resourceIntents": []
            },
            "prompt": "test",
            "limits": {"maxLanes": 2, "laneTimeoutMs": 1000},
            "projectId": "guest-must-not-supply-this"
        });
        assert!(serde_json::from_value::<CandidateLaneLaunchRequest>(request).is_err());
    }

    #[tokio::test]
    async fn validator_tip_change_is_rejected_and_cleaned() {
        let root = TempDir::new().expect("temp root");
        let (mut orchestrator, mut request) = request_for(&root, 1);
        request.guest.acceptance.validation_commands = vec![ValidationCommand::new(
            "git",
            ["commit", "--allow-empty", "--quiet", "-m", "validator"],
        )];
        let report = orchestrator
            .execute_candidate_set(
                request,
                root.path().join("lanes"),
                &fake_executor(),
                CancellationToken::new(),
            )
            .await
            .expect("validator failure is a terminal lane result");
        assert_eq!(report.outcomes[0].state, LaneState::Failed);
        assert!(
            orchestrator
                .git()
                .list_candidate_ref_names()
                .expect("refs")
                .is_empty()
        );
    }
    #[tokio::test]
    async fn executor_failure_cleans_only_failed_lane() {
        let root = TempDir::new().expect("temp root");
        let (mut orchestrator, request) = request_for(&root, 2);
        let mut executor = fake_executor();
        executor.fail_on = Some(1);
        let report = orchestrator
            .execute_candidate_set(
                request,
                root.path().join("lanes"),
                &executor,
                CancellationToken::new(),
            )
            .await
            .expect("execute fake lanes");
        assert_eq!(report.submitted_count(), 1);
        assert_eq!(
            report
                .outcomes
                .iter()
                .filter(|outcome| outcome.state == LaneState::Failed)
                .count(),
            1
        );
        assert_eq!(
            orchestrator
                .git()
                .list_candidate_ref_names()
                .expect("refs")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn cancellation_abandons_all_lanes_and_removes_worktrees() {
        let root = TempDir::new().expect("temp root");
        let (mut orchestrator, request) = request_for(&root, 2);
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let error = orchestrator
            .execute_candidate_set(
                request,
                root.path().join("lanes"),
                &fake_executor(),
                cancellation,
            )
            .await
            .expect_err("cancelled execution");
        assert!(matches!(
            error,
            OrchestrationError::ExecutionCancelled { .. }
        ));
        assert!(
            orchestrator
                .git()
                .list_candidate_ref_names()
                .expect("refs")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn in_flight_cancellation_requires_executor_acknowledgement() {
        let root = TempDir::new().expect("temp root");
        let (mut orchestrator, request) = request_for(&root, 1);
        let cancellation = CancellationToken::new();
        let executor = CancellationAcknowledgingExecutor {
            started: Arc::new(Notify::new()),
            cancel_calls: Arc::new(AtomicUsize::new(0)),
            released: Arc::new(Notify::new()),
        };
        let started = Arc::clone(&executor.started);
        let cancel_calls = Arc::clone(&executor.cancel_calls);
        let execution = orchestrator.execute_candidate_set(
            request,
            root.path().join("lanes"),
            &executor,
            cancellation.clone(),
        );
        tokio::pin!(execution);
        tokio::select! {
            _ = started.notified() => cancellation.cancel(),
            result = &mut execution => panic!("execution completed before cancellation: {result:?}"),
        }
        let error = execution.await.expect_err("cancelled execution");
        assert!(matches!(
            error,
            OrchestrationError::ExecutionCancelled { .. }
        ));
        assert_eq!(cancel_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn timeout_cleans_timed_out_lane() {
        let root = TempDir::new().expect("temp root");
        let (mut orchestrator, mut request) = request_for(&root, 1);
        request.guest.limits.lane_timeout_ms = 1;
        let mut executor = fake_executor();
        executor.delay = Some(Duration::from_millis(50));
        let report = orchestrator
            .execute_candidate_set(
                request,
                root.path().join("lanes"),
                &executor,
                CancellationToken::new(),
            )
            .await
            .expect("timeout is a terminal report");
        assert_eq!(report.outcomes[0].state, LaneState::TimedOut);
        assert!(
            orchestrator
                .git()
                .list_candidate_ref_names()
                .expect("refs")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn unacknowledged_termination_quarantines_every_lane_and_recovers_after_restart() {
        let root = TempDir::new().expect("temp root");
        let (mut orchestrator, mut request) = request_for(&root, 2);
        request.guest.limits.lane_timeout_ms = 1;
        let executor = NonAcknowledgingExecutor {
            cancel_calls: Arc::new(AtomicUsize::new(0)),
        };

        let report = orchestrator
            .execute_candidate_set(
                request,
                root.path().join("lanes"),
                &executor,
                CancellationToken::new(),
            )
            .await
            .expect("unacknowledged termination is durably quarantined");
        assert_eq!(report.outcomes.len(), 2);
        assert!(
            report
                .outcomes
                .iter()
                .all(|outcome| outcome.state == LaneState::Quarantined)
        );
        assert_eq!(executor.cancel_calls.load(Ordering::SeqCst), 2);

        let status = orchestrator
            .status(report.candidate_set_id, 8)
            .expect("quarantined status");
        assert_eq!(status.counts.quarantined, 2);
        let inventory = orchestrator
            .recovery_inventory(8)
            .expect("recovery inventory");
        assert_eq!(inventory.lanes.len(), 2);
        assert!(inventory.lanes.iter().all(|lane| {
            lane.state == LaneState::Quarantined
                && lane.cleanup_state == crate::LaneCleanupState::Pending
                && lane.worktree.path.is_dir()
        }));
        assert_eq!(
            orchestrator
                .git()
                .list_candidate_ref_names()
                .expect("retained refs")
                .len(),
            2
        );

        let restarted = reopen_orchestrator(&root);
        let recovered = restarted
            .recovery_inventory(8)
            .expect("recovery inventory survives restart");
        assert_eq!(recovered.lanes.len(), 2);
        let candidate_ids = recovered
            .lanes
            .iter()
            .map(|lane| lane.candidate_id)
            .collect::<Vec<_>>();
        for candidate_id in &candidate_ids {
            restarted
                .cleanup_lane(*candidate_id)
                .expect("recover quarantined lane");
        }
        for candidate_id in candidate_ids {
            restarted
                .cleanup_lane(candidate_id)
                .expect("cleanup is idempotent after restart");
        }
        assert!(
            restarted
                .recovery_inventory(8)
                .expect("empty recovery inventory")
                .lanes
                .is_empty()
        );
        assert!(
            restarted
                .git()
                .list_candidate_ref_names()
                .expect("cleaned refs")
                .is_empty()
        );
        assert!(
            recovered
                .lanes
                .iter()
                .all(|lane| !lane.worktree.path.exists())
        );
    }
}
