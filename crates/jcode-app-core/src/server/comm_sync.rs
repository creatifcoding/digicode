use super::{
    ClientConnectionInfo, FileTouchService, SwarmEvent, SwarmEventType, SwarmMember, SwarmState,
    VersionedPlan, broadcast_swarm_plan, persist_swarm_state_for, record_swarm_event,
};
use crate::agent::Agent;
use crate::protocol::{
    AgentStatusSnapshot, NotificationType, PlanGraphStatus, ServerEvent, SessionActivitySnapshot,
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

const MAX_GRAPH_ARTIFACT_BYTES: usize = 128 * 1024;

type SessionAgents = Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>;

pub(super) struct CommResyncPlanContext<'a> {
    pub(super) client_event_tx: &'a mpsc::UnboundedSender<ServerEvent>,
    pub(super) swarm_members: &'a Arc<RwLock<HashMap<String, SwarmMember>>>,
    pub(super) swarms_by_id: &'a Arc<RwLock<HashMap<String, HashSet<String>>>>,
    pub(super) swarm_plans: &'a Arc<RwLock<HashMap<String, VersionedPlan>>>,
    pub(super) swarm_coordinators: &'a Arc<RwLock<HashMap<String, String>>>,
    pub(super) event_history: &'a Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    pub(super) event_counter: &'a Arc<std::sync::atomic::AtomicU64>,
    pub(super) swarm_event_tx: &'a broadcast::Sender<SwarmEvent>,
}

fn live_activity_snapshot(
    connections: &HashMap<String, ClientConnectionInfo>,
    session_id: &str,
    fallback_processing: bool,
) -> Option<SessionActivitySnapshot> {
    let mut processing_without_tool = false;
    let mut tool_name = None;
    for info in connections.values() {
        if info.session_id != session_id || !info.is_processing {
            continue;
        }
        if let Some(current_tool_name) = info.current_tool_name.clone() {
            tool_name = Some(current_tool_name);
            break;
        }
        processing_without_tool = true;
    }

    tool_name
        .map(|current_tool_name| SessionActivitySnapshot {
            is_processing: true,
            current_tool_name: Some(current_tool_name),
        })
        .or_else(|| {
            processing_without_tool.then_some(SessionActivitySnapshot {
                is_processing: true,
                current_tool_name: None,
            })
        })
        .or_else(|| {
            fallback_processing.then_some(SessionActivitySnapshot {
                is_processing: true,
                current_tool_name: None,
            })
        })
}

/// Recent-token lookback window used when reporting per-agent churn in
/// `swarm list`. Short enough to reflect "what is this agent doing right now".
pub(super) const SWARM_LIST_TOKEN_WINDOW_SECS: u64 = 10;

/// Runtime extras for a swarm member, gathered without holding the agent lock
/// for long. Used to enrich the `swarm list` roster with live activity,
/// provider/model, token churn, turn count, and todo progress.
#[derive(Default)]
pub(super) struct MemberRuntimeExtras {
    pub(super) activity: Option<SessionActivitySnapshot>,
    pub(super) provider_name: Option<String>,
    pub(super) provider_model: Option<String>,
    pub(super) turn_count: Option<u64>,
    pub(super) recent_total_tokens: Option<u64>,
    pub(super) recent_output_tokens: Option<u64>,
    pub(super) recent_window_secs: Option<u64>,
    pub(super) cumulative_total_tokens: Option<u64>,
    pub(super) last_activity_age_secs: Option<u64>,
    pub(super) todos_completed: Option<usize>,
    pub(super) todos_total: Option<usize>,
}

/// Gather live runtime extras for a single member session.
///
/// `member_is_running` is used as a fallback "processing" hint when no live
/// client connection is reporting activity (e.g. headless sessions).
pub(super) async fn member_runtime_extras(
    session_id: &str,
    member_is_running: bool,
    sessions: &SessionAgents,
    client_connections: &Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
) -> MemberRuntimeExtras {
    let activity = {
        let connections = client_connections.read().await;
        live_activity_snapshot(&connections, session_id, member_is_running)
    };

    let (provider_name, provider_model) = {
        let agent_sessions = sessions.read().await;
        if let Some(agent) = agent_sessions.get(session_id) {
            // Never block on a busy agent: token churn and turns come from the
            // lock-free metrics registry, so a missing provider name here just
            // means the agent is mid-turn.
            if let Ok(agent) = agent.try_lock() {
                (Some(agent.provider_name()), Some(agent.provider_model()))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    };

    let metrics = crate::session_metrics::snapshot(
        session_id,
        std::time::Duration::from_secs(SWARM_LIST_TOKEN_WINDOW_SECS),
    );

    let (todos_completed, todos_total) = match crate::todo::load_todos(session_id) {
        Ok(todos) if !todos.is_empty() => {
            let completed = todos.iter().filter(|t| t.status == "completed").count();
            (Some(completed), Some(todos.len()))
        }
        _ => (None, None),
    };

    MemberRuntimeExtras {
        activity,
        provider_name,
        provider_model,
        turn_count: metrics.map(|m| m.turns),
        recent_total_tokens: metrics.map(|m| m.recent_total_tokens),
        recent_output_tokens: metrics.map(|m| m.recent_output_tokens),
        recent_window_secs: metrics.map(|_| SWARM_LIST_TOKEN_WINDOW_SECS),
        cumulative_total_tokens: metrics.map(|m| m.cumulative_total_tokens),
        last_activity_age_secs: metrics.and_then(|m| m.last_activity_age_secs),
        todos_completed,
        todos_total,
    }
}

async fn ensure_same_swarm_access(
    id: u64,
    req_session_id: &str,
    target_session: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) -> bool {
    let (req_swarm, target_swarm) = {
        let members = swarm_members.read().await;
        (
            members
                .get(req_session_id)
                .and_then(|member| member.swarm_id.clone()),
            members
                .get(target_session)
                .and_then(|member| member.swarm_id.clone()),
        )
    };

    if req_swarm.is_some() && req_swarm == target_swarm {
        true
    } else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: format!(
                "Session '{}' is not in the same swarm as requester '{}'",
                target_session, req_session_id
            ),
            retry_after_secs: None,
        });
        false
    }
}

async fn can_read_full_context(
    req_session_id: &str,
    target_session: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> bool {
    if req_session_id == target_session {
        return true;
    }

    let members = swarm_members.read().await;
    members
        .get(req_session_id)
        .map(|member| member.role == "coordinator")
        .unwrap_or(false)
}

pub(super) async fn handle_comm_summary(
    id: u64,
    req_session_id: String,
    target_session: String,
    limit: Option<usize>,
    sessions: &SessionAgents,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    if !ensure_same_swarm_access(
        id,
        &req_session_id,
        &target_session,
        swarm_members,
        client_event_tx,
    )
    .await
    {
        return;
    }

    let limit = limit.unwrap_or(10);
    let agent_sessions = sessions.read().await;
    if let Some(agent) = agent_sessions.get(&target_session) {
        let tool_calls = if let Ok(agent) = agent.try_lock() {
            agent.get_tool_call_summaries(limit)
        } else {
            let _ = client_event_tx.send(ServerEvent::Error {
                id,
                message: format!(
                    "Session '{}' is busy; try summary again shortly",
                    target_session
                ),
                retry_after_secs: Some(1),
            });
            return;
        };
        let _ = client_event_tx.send(ServerEvent::CommSummaryResponse {
            id,
            session_id: target_session,
            tool_calls,
        });
    } else {
        let _ = client_event_tx.send(ServerEvent::CommSummaryResponse {
            id,
            session_id: target_session,
            tool_calls: Vec::new(),
        });
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "status snapshots combine live connection state, session metadata, files touched, and optional provider/model hints"
)]
pub(super) async fn handle_comm_status(
    id: u64,
    req_session_id: String,
    target_session: String,
    sessions: &SessionAgents,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    client_connections: &Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
    file_touch: &FileTouchService,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    if !ensure_same_swarm_access(
        id,
        &req_session_id,
        &target_session,
        swarm_members,
        client_event_tx,
    )
    .await
    {
        return;
    }

    let snapshot = {
        let members = swarm_members.read().await;
        let Some(member) = members.get(&target_session) else {
            let _ = client_event_tx.send(ServerEvent::Error {
                id,
                message: format!("Unknown session '{target_session}'"),
                retry_after_secs: None,
            });
            return;
        };

        let files_touched = file_touch
            .sorted_file_strings_for_session(&target_session)
            .await;

        let activity = {
            let connections = client_connections.read().await;
            live_activity_snapshot(&connections, &target_session, member.status == "running")
        };

        let (provider_name, provider_model) = {
            let agent_sessions = sessions.read().await;
            if let Some(agent) = agent_sessions.get(&target_session) {
                if let Ok(agent) = agent.try_lock() {
                    (Some(agent.provider_name()), Some(agent.provider_model()))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        };

        AgentStatusSnapshot {
            session_id: member.session_id.clone(),
            friendly_name: member.friendly_name.clone(),
            swarm_id: member.swarm_id.clone(),
            status: Some(member.status.clone()),
            detail: member.detail.clone(),
            role: Some(member.role.clone()),
            is_headless: Some(member.is_headless),
            live_attachments: Some(member.event_txs.len()),
            status_age_secs: Some(member.last_status_change.elapsed().as_secs()),
            last_activity_age_secs: crate::session_metrics::last_activity_age_secs(&target_session),
            joined_age_secs: Some(member.joined_at.elapsed().as_secs()),
            files_touched,
            activity,
            provider_name,
            provider_model,
        }
    };

    let _ = client_event_tx.send(ServerEvent::CommStatusResponse { id, snapshot });
}

pub(super) async fn handle_comm_read_context(
    id: u64,
    req_session_id: String,
    target_session: String,
    sessions: &SessionAgents,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    if !ensure_same_swarm_access(
        id,
        &req_session_id,
        &target_session,
        swarm_members,
        client_event_tx,
    )
    .await
    {
        return;
    }

    if !can_read_full_context(&req_session_id, &target_session, swarm_members).await {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Only the coordinator, worktree manager, or the target session may read full context. Use summary for lightweight access.".to_string(),
            retry_after_secs: None,
        });
        return;
    }

    let agent_sessions = sessions.read().await;
    if let Some(agent) = agent_sessions.get(&target_session) {
        let messages = if let Ok(agent) = agent.try_lock() {
            agent.get_history()
        } else {
            let _ = client_event_tx.send(ServerEvent::Error {
                id,
                message: format!(
                    "Session '{}' is busy; try read_context again shortly",
                    target_session
                ),
                retry_after_secs: Some(1),
            });
            return;
        };
        let _ = client_event_tx.send(ServerEvent::CommContextHistory {
            id,
            session_id: target_session,
            messages,
        });
    } else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: format!("Unknown session '{target_session}'"),
            retry_after_secs: None,
        });
    }
}

pub(super) async fn handle_comm_plan_status(
    id: u64,
    req_session_id: String,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let swarm_id = {
        let members = swarm_members.read().await;
        members
            .get(&req_session_id)
            .and_then(|member| member.swarm_id.clone())
    };

    let Some(swarm_id) = swarm_id else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Not in a swarm.".to_string(),
            retry_after_secs: None,
        });
        return;
    };

    let summary = {
        let plans = swarm_plans.read().await;
        let plan = plans.get(&swarm_id);
        if let Some(plan) = plan {
            PlanGraphStatus::from_versioned_plan(swarm_id.clone(), plan, Some(8), Vec::new())
        } else {
            PlanGraphStatus::empty_for_swarm(swarm_id.clone())
        }
    };

    let _ = client_event_tx.send(ServerEvent::CommPlanStatusResponse { id, summary });
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated: String = value.chars().take(max_chars).collect();
    truncated.push('…');
    truncated
}

fn graph_node_payload(
    plan: &VersionedPlan,
    node_id: &str,
    include_artifact: bool,
) -> Option<Value> {
    let item = plan.items.iter().find(|item| item.id == node_id)?;
    let dependents: Vec<&str> = plan
        .items
        .iter()
        .filter(|candidate| candidate.blocked_by.iter().any(|dep| dep == node_id))
        .map(|candidate| candidate.id.as_str())
        .collect();
    let meta = plan.node_meta.get(node_id);
    let artifact_raw = meta.and_then(|meta| meta.artifact_json.as_deref());
    let artifact_too_large = artifact_raw.is_some_and(|raw| raw.len() > MAX_GRAPH_ARTIFACT_BYTES);
    let artifact = artifact_raw
        .filter(|raw| raw.len() <= MAX_GRAPH_ARTIFACT_BYTES)
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    let artifact_invalid = artifact_raw.is_some() && !artifact_too_large && artifact.is_none();
    let item = json!({
        "id": item.id,
        "content": truncate_chars(&item.content, if include_artifact { 8_000 } else { 1_000 }),
        "status": item.status,
        "priority": item.priority,
        "subsystem": item.subsystem,
        "file_scope": item.file_scope.iter().take(50).collect::<Vec<_>>(),
        "blocked_by": item.blocked_by,
        "assigned_to": item.assigned_to,
    });
    Some(json!({
        "item": item,
        "kind": meta.and_then(|meta| meta.kind.as_deref()),
        "parent": meta.and_then(|meta| meta.parent.as_deref()),
        "origin": meta.and_then(|meta| meta.origin.as_deref()),
        "expanded": meta.is_some_and(|meta| meta.expanded),
        "is_gate": meta.is_some_and(|meta| meta.is_gate),
        "planner": meta.and_then(|meta| meta.planner.as_deref()),
        "dependents": dependents,
        "progress": plan.task_progress.get(node_id),
        "artifact_present": artifact_raw.is_some(),
        "artifact_invalid": artifact_invalid,
        "artifact_too_large": artifact_too_large,
        "artifact_confidence": artifact.as_ref().and_then(|value| value.get("confidence")),
        "artifact": include_artifact.then_some(artifact).flatten(),
    }))
}

pub(super) fn graph_read_payload(
    swarm_id: &str,
    plan: &VersionedPlan,
    action: &str,
    node_id: Option<&str>,
    requested_limit: Option<usize>,
) -> Result<Value, String> {
    let limit = requested_limit.unwrap_or(100).clamp(1, 500);
    match action {
        "graph_show" => {
            let summary = PlanGraphStatus::from_versioned_plan(
                swarm_id.to_string(),
                plan,
                Some(8),
                Vec::new(),
            );
            let terminal = summary.active_ids.is_empty()
                && summary.ready_ids.is_empty()
                && summary.blocked_ids.is_empty();
            let success = summary.item_count > 0
                && terminal
                && summary.failed_ids.is_empty()
                && summary.cycle_ids.is_empty()
                && summary.unresolved_dependency_ids.is_empty()
                && summary.completed_ids.len() == summary.item_count;
            let nodes: Vec<Value> = plan
                .items
                .iter()
                .take(limit)
                .filter_map(|item| graph_node_payload(plan, &item.id, false))
                .collect();
            let returned_nodes = nodes.len();
            Ok(json!({
                "summary": summary,
                "terminal": terminal,
                "success": success,
                "nodes": nodes,
                "returned_nodes": returned_nodes,
                "truncated": plan.items.len() > limit,
            }))
        }
        "node_show" => {
            let node_id = node_id.ok_or_else(|| "node_show requires node_id".to_string())?;
            graph_node_payload(plan, node_id, true)
                .ok_or_else(|| format!("Unknown graph node '{node_id}'"))
        }
        "artifact_get" => {
            let node_id = node_id.ok_or_else(|| "artifact_get requires node_id".to_string())?;
            let item = plan
                .items
                .iter()
                .find(|item| item.id == node_id)
                .ok_or_else(|| format!("Unknown graph node '{node_id}'"))?;
            let meta = plan.node_meta.get(node_id);
            let raw = meta
                .and_then(|meta| meta.artifact_json.as_deref())
                .ok_or_else(|| format!("Node '{node_id}' has no artifact"))?;
            if raw.len() > MAX_GRAPH_ARTIFACT_BYTES {
                return Err(format!(
                    "Node '{node_id}' artifact is too large to return ({} bytes; max {})",
                    raw.len(),
                    MAX_GRAPH_ARTIFACT_BYTES
                ));
            }
            let artifact = serde_json::from_str::<Value>(raw)
                .map_err(|error| format!("Node '{node_id}' has invalid artifact JSON: {error}"))?;
            Ok(json!({
                "node_id": node_id,
                "status": item.status,
                "kind": meta.and_then(|meta| meta.kind.as_deref()),
                "artifact": artifact,
            }))
        }
        "artifact_list" => {
            let mut invalid_count = 0usize;
            let all_artifacts: Vec<Value> = plan
                .items
                .iter()
                .filter_map(|item| {
                    let meta = plan.node_meta.get(&item.id)?;
                    let raw = meta.artifact_json.as_deref()?;
                    if raw.len() > MAX_GRAPH_ARTIFACT_BYTES {
                        invalid_count += 1;
                        return None;
                    }
                    let artifact = match serde_json::from_str::<Value>(raw) {
                        Ok(artifact) => artifact,
                        Err(_) => {
                            invalid_count += 1;
                            return None;
                        }
                    };
                    Some(json!({
                        "node_id": item.id,
                        "status": item.status,
                        "kind": meta.kind.as_deref(),
                        "confidence": artifact.get("confidence"),
                        "findings": artifact.get("findings").and_then(Value::as_str).map(|value| truncate_chars(value, 240)),
                        "validation": artifact.get("validation").and_then(Value::as_str).map(|value| truncate_chars(value, 240)),
                    }))
                })
                .collect();
            let total = all_artifacts.len();
            let artifacts: Vec<Value> = all_artifacts.into_iter().take(limit).collect();
            let returned = artifacts.len();
            Ok(json!({
                "artifacts": artifacts,
                "returned": returned,
                "total": total,
                "invalid_count": invalid_count,
                "truncated": total > limit,
            }))
        }
        "hydration_preview" => {
            let node_id =
                node_id.ok_or_else(|| "hydration_preview requires node_id".to_string())?;
            let item = plan
                .items
                .iter()
                .find(|item| item.id == node_id)
                .ok_or_else(|| format!("Unknown graph node '{node_id}'"))?;
            let mut included = Vec::new();
            let mut missing = Vec::new();
            for dependency in &item.blocked_by {
                let Some(dep) = plan
                    .items
                    .iter()
                    .find(|candidate| &candidate.id == dependency)
                else {
                    missing.push(json!({"node_id": dependency, "reason": "unknown_dependency"}));
                    continue;
                };
                if !jcode_plan::is_completed_status(&dep.status) {
                    missing.push(json!({"node_id": dependency, "reason": "not_completed", "status": dep.status}));
                    continue;
                }
                let Some(raw) = plan
                    .node_meta
                    .get(dependency)
                    .and_then(|meta| meta.artifact_json.as_deref())
                else {
                    missing.push(json!({"node_id": dependency, "reason": "artifact_missing"}));
                    continue;
                };
                if raw.len() > MAX_GRAPH_ARTIFACT_BYTES
                    || serde_json::from_str::<jcode_plan::dag::HandoffArtifact>(raw).is_err()
                {
                    missing.push(json!({"node_id": dependency, "reason": "artifact_invalid"}));
                } else {
                    included.push(dependency.clone());
                }
            }
            let context = jcode_plan::bridge::upstream_context(plan, node_id);
            let context_truncated = context
                .as_deref()
                .is_some_and(|value| value.chars().count() > 32_000);
            let context = context.map(|value| truncate_chars(&value, 32_000));
            let ready = missing.is_empty();
            Ok(json!({
                "node_id": node_id,
                "dependency_ids": item.blocked_by,
                "included_artifact_ids": included,
                "missing_inputs": missing,
                "ready": ready,
                "context": context,
                "context_truncated": context_truncated,
            }))
        }
        _ => Err(format!("Unknown graph read action '{action}'")),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_comm_graph_read(
    id: u64,
    req_session_id: String,
    action: String,
    node_id: Option<String>,
    limit: Option<usize>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let swarm_id = {
        let members = swarm_members.read().await;
        members
            .get(&req_session_id)
            .and_then(|member| member.swarm_id.clone())
    };
    let Some(swarm_id) = swarm_id else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Not in a swarm.".to_string(),
            retry_after_secs: None,
        });
        return;
    };
    let result = {
        let plans = swarm_plans.read().await;
        match plans.get(&swarm_id) {
            Some(plan) => graph_read_payload(&swarm_id, plan, &action, node_id.as_deref(), limit),
            None if action == "graph_show" || action == "artifact_list" => graph_read_payload(
                &swarm_id,
                &VersionedPlan::new(),
                &action,
                node_id.as_deref(),
                limit,
            ),
            None => Err("No swarm plan exists for this swarm.".to_string()),
        }
    };
    match result {
        Ok(payload) => {
            let _ = client_event_tx.send(ServerEvent::CommGraphReadResponse {
                id,
                action,
                payload,
            });
        }
        Err(message) => {
            let _ = client_event_tx.send(ServerEvent::Error {
                id,
                message,
                retry_after_secs: None,
            });
        }
    }
}

pub(super) async fn handle_comm_resync_plan(
    id: u64,
    req_session_id: String,
    ctx: &CommResyncPlanContext<'_>,
) {
    let swarm_id = {
        let members = ctx.swarm_members.read().await;
        members
            .get(&req_session_id)
            .and_then(|member| member.swarm_id.clone())
    };

    if let Some(swarm_id) = swarm_id {
        let plan_state = {
            let mut plans = ctx.swarm_plans.write().await;
            plans.get_mut(&swarm_id).map(|plan| {
                plan.participants.insert(req_session_id.clone());
                (plan.version, plan.items.len())
            })
        };
        if let Some((version, item_count)) = plan_state {
            let swarm_state = SwarmState {
                members: Arc::clone(ctx.swarm_members),
                swarms_by_id: Arc::clone(ctx.swarms_by_id),
                plans: Arc::clone(ctx.swarm_plans),
                coordinators: Arc::clone(ctx.swarm_coordinators),
            };
            persist_swarm_state_for(&swarm_id, &swarm_state).await;
            if let Some(member) = ctx.swarm_members.read().await.get(&req_session_id) {
                let _ = member.event_tx.send(ServerEvent::Notification {
                    from_session: req_session_id.clone(),
                    from_name: member.friendly_name.clone(),
                    notification_type: NotificationType::Message {
                        scope: Some("plan".to_string()),
                        channel: None,
                        tldr: None,
                    },
                    message: format!(
                        "Plan attached to this session (v{}, {} items).",
                        version, item_count
                    ),
                });
            }
            broadcast_swarm_plan(
                &swarm_id,
                Some("resync".to_string()),
                ctx.swarm_plans,
                ctx.swarm_members,
                ctx.swarms_by_id,
            )
            .await;
            record_swarm_event(
                ctx.event_history,
                ctx.event_counter,
                ctx.swarm_event_tx,
                req_session_id.clone(),
                None,
                Some(swarm_id.clone()),
                SwarmEventType::PlanUpdate {
                    swarm_id: swarm_id.clone(),
                    item_count,
                },
            )
            .await;
            let _ = ctx.client_event_tx.send(ServerEvent::Done { id });
        } else {
            let _ = ctx.client_event_tx.send(ServerEvent::Error {
                id,
                message: "No swarm plan exists for this swarm.".to_string(),
                retry_after_secs: None,
            });
        }
    } else {
        let _ = ctx.client_event_tx.send(ServerEvent::Error {
            id,
            message: "Not in a swarm.".to_string(),
            retry_after_secs: None,
        });
    }
}

#[cfg(test)]
mod graph_read_tests {
    use super::*;
    use jcode_plan::{NodeMeta, PlanItem, SwarmTaskProgress};

    fn test_plan() -> VersionedPlan {
        let mut plan = VersionedPlan::new();
        plan.version = 55;
        plan.mode = "deep".to_string();
        plan.items = vec![
            PlanItem {
                content: "audit the store".to_string(),
                status: "completed".to_string(),
                priority: "high".to_string(),
                id: "audit.store".to_string(),
                subsystem: None,
                file_scope: vec!["store.rs".to_string()],
                blocked_by: Vec::new(),
                assigned_to: Some("worker-a".to_string()),
            },
            PlanItem {
                content: "audit tool context".to_string(),
                status: "failed".to_string(),
                priority: "high".to_string(),
                id: "audit.toolctx".to_string(),
                subsystem: None,
                file_scope: Vec::new(),
                blocked_by: Vec::new(),
                assigned_to: Some("worker-b".to_string()),
            },
            PlanItem {
                content: "implement canonical store".to_string(),
                status: "todo".to_string(),
                priority: "high".to_string(),
                id: "implement.store".to_string(),
                subsystem: None,
                file_scope: vec!["store.rs".to_string()],
                blocked_by: vec!["audit.store".to_string(), "audit.toolctx".to_string()],
                assigned_to: None,
            },
        ];
        plan.node_meta.insert(
            "audit.store".to_string(),
            NodeMeta {
                kind: Some("explore".to_string()),
                artifact_json: Some(
                    json!({
                        "findings": "SQLite is canonical.",
                        "evidence": ["store.rs:10"],
                        "validation": "read-only audit",
                        "confidence": "high",
                        "what_i_did_not_check": []
                    })
                    .to_string(),
                ),
                ..NodeMeta::default()
            },
        );
        plan.task_progress.insert(
            "audit.toolctx".to_string(),
            SwarmTaskProgress {
                checkpoint_summary: Some("ENOSPC".to_string()),
                ..SwarmTaskProgress::default()
            },
        );
        plan
    }

    #[test]
    fn graph_show_exposes_false_success_and_bounded_nodes() {
        let payload = graph_read_payload("swarm-1", &test_plan(), "graph_show", None, Some(2))
            .expect("graph payload");
        assert_eq!(payload["success"], false);
        assert_eq!(payload["terminal"], false);
        assert_eq!(payload["truncated"], true);
        assert_eq!(payload["returned_nodes"], 2);
        assert_eq!(payload["summary"]["failed_ids"], json!(["audit.toolctx"]));
        assert!(payload["nodes"][0]["artifact"].is_null());
    }

    #[test]
    fn node_and_artifact_reads_preserve_relations_and_typed_payload() {
        let plan = test_plan();
        let node = graph_read_payload(&"swarm-1", &plan, "node_show", Some("audit.store"), None)
            .expect("node payload");
        assert_eq!(node["dependents"], json!(["implement.store"]));
        assert_eq!(node["artifact_present"], true);
        assert_eq!(node["artifact"]["confidence"], "high");

        let artifact =
            graph_read_payload("swarm-1", &plan, "artifact_get", Some("audit.store"), None)
                .expect("artifact payload");
        assert_eq!(artifact["artifact"]["findings"], "SQLite is canonical.");
    }

    #[test]
    fn hydration_preview_matches_forward_dataflow_and_names_missing_inputs() {
        let plan = test_plan();
        let payload = graph_read_payload(
            "swarm-1",
            &plan,
            "hydration_preview",
            Some("implement.store"),
            None,
        )
        .expect("hydration payload");
        assert_eq!(payload["ready"], false);
        assert_eq!(payload["included_artifact_ids"], json!(["audit.store"]));
        assert_eq!(payload["missing_inputs"][0]["node_id"], "audit.toolctx");
        assert_eq!(payload["missing_inputs"][0]["reason"], "not_completed");
        let expected = jcode_plan::bridge::upstream_context(&plan, "implement.store")
            .expect("upstream context");
        assert_eq!(payload["context"], expected);
    }

    #[test]
    fn artifact_list_is_summary_only_and_bounded() {
        let payload = graph_read_payload("swarm-1", &test_plan(), "artifact_list", None, Some(1))
            .expect("artifact list");
        assert_eq!(payload["returned"], 1);
        assert_eq!(payload["total"], 1);
        assert_eq!(payload["artifacts"][0]["findings"], "SQLite is canonical.");
        assert!(payload["artifacts"][0].get("artifact").is_none());
    }

    #[test]
    fn hydration_preview_rejects_invalid_artifacts_that_runtime_hydration_skips() {
        let mut plan = test_plan();
        plan.node_meta
            .get_mut("audit.store")
            .expect("meta")
            .artifact_json = Some("not-json".to_string());
        let payload = graph_read_payload(
            "swarm-1",
            &plan,
            "hydration_preview",
            Some("implement.store"),
            None,
        )
        .expect("hydration payload");
        assert_eq!(payload["ready"], false);
        assert_eq!(payload["missing_inputs"][0]["reason"], "artifact_invalid");
        assert!(payload["context"].is_null());
    }
}
