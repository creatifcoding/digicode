use super::{SessionAgents, SwarmMember};
use crate::protocol::SessionListEntry;
use crate::session::{Session, SessionStatus, session_journal_path_from_snapshot};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, SecondsFormat, Utc};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

pub(super) async fn handle_list_sessions_request(
    id: u64,
    client_event_tx: &mpsc::UnboundedSender<crate::protocol::ServerEvent>,
    sessions: &SessionAgents,
    global_session_id: &Arc<RwLock<String>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Result<()> {
    let event = match list_resumable_sessions(sessions, global_session_id, swarm_members).await {
        Ok(sessions) => crate::protocol::ServerEvent::SessionList { id, sessions },
        Err(error) => crate::protocol::ServerEvent::Error {
            id,
            message: format!("Failed to list sessions: {error}"),
            retry_after_secs: None,
        },
    };
    client_event_tx
        .send(event)
        .map_err(|_| anyhow!("session discovery client disconnected"))
}

pub(super) async fn list_resumable_sessions(
    sessions: &SessionAgents,
    global_session_id: &Arc<RwLock<String>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Result<Vec<SessionListEntry>> {
    let mut live_session_ids: HashSet<String> = sessions.read().await.keys().cloned().collect();
    live_session_ids.extend(
        crate::storage::session_presence()
            .into_iter()
            .map(|presence| presence.session_id),
    );
    let current_session_id = global_session_id.read().await.clone();
    let friendly_names = swarm_members
        .read()
        .await
        .iter()
        .filter_map(|(session_id, member)| {
            member
                .friendly_name
                .clone()
                .map(|name| (session_id.clone(), name))
        })
        .collect();

    tokio::task::spawn_blocking(move || {
        collect_resumable_sessions(&current_session_id, &live_session_ids, &friendly_names)
    })
    .await
    .context("session discovery task failed")?
}

fn collect_resumable_sessions(
    current_session_id: &str,
    live_session_ids: &HashSet<String>,
    friendly_names: &HashMap<String, String>,
) -> Result<Vec<SessionListEntry>> {
    let sessions_dir = crate::storage::jcode_dir()?.join("sessions");
    if !sessions_dir.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&sessions_dir).context("read sessions directory")? {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Some(session_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Ok(session) = Session::load_startup_stub(session_id) else {
            continue;
        };
        if session.is_debug {
            continue;
        }

        let updated_at = latest_persisted_update(&path, session.updated_at)?;
        let friendly_name = friendly_names
            .get(session_id)
            .cloned()
            .or_else(|| non_empty(session.short_name.clone()));
        sessions.push(SessionListEntry {
            session_id: session.id.clone(),
            title: session.display_title().map(ToOwned::to_owned),
            friendly_name,
            working_dir: non_empty(session.working_dir.clone()),
            updated_at: updated_at.to_rfc3339_opts(SecondsFormat::Millis, true),
            status: session_status(&session.status).to_string(),
            is_current: session.id == current_session_id,
            is_live: live_session_ids.contains(&session.id),
        });
    }

    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    Ok(sessions)
}

fn latest_persisted_update(path: &Path, fallback: DateTime<Utc>) -> Result<DateTime<Utc>> {
    let mut updated_at = fallback;
    for candidate in [path.to_path_buf(), session_journal_path_from_snapshot(path)] {
        if !candidate.exists() {
            continue;
        }
        let modified = std::fs::metadata(&candidate)
            .with_context(|| format!("read session metadata for {}", candidate.display()))?
            .modified()
            .with_context(|| format!("read session mtime for {}", candidate.display()))?;
        updated_at = updated_at.max(modified.into());
    }
    Ok(updated_at)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn session_status(status: &SessionStatus) -> &'static str {
    match status {
        SessionStatus::Active => "active",
        SessionStatus::Closed => "closed",
        SessionStatus::Crashed { .. } => "crashed",
        SessionStatus::Reloaded => "reloaded",
        SessionStatus::Compacted => "compacted",
        SessionStatus::RateLimited => "rate_limited",
        SessionStatus::Error { .. } => "error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ContentBlock, Role};
    use crate::session::{SessionStatus, StoredMessage};

    struct TestHome {
        previous: Option<std::ffi::OsString>,
        _temp: tempfile::TempDir,
    }

    impl TestHome {
        fn new() -> Self {
            let temp = tempfile::TempDir::new().expect("temp JCODE_HOME");
            let previous = std::env::var_os("JCODE_HOME");
            crate::env::set_var("JCODE_HOME", temp.path());
            Self {
                previous,
                _temp: temp,
            }
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(previous) => crate::env::set_var("JCODE_HOME", previous),
                None => crate::env::remove_var("JCODE_HOME"),
            }
        }
    }

    #[tokio::test]
    async fn list_sessions_returns_startup_metadata_before_subscribe() {
        assert!(
            (crate::protocol::Request::ListSessions { id: 1 }).is_lightweight_control_request()
        );
        let _env_lock = crate::storage::lock_test_env();
        let _home = TestHome::new();
        let current_session_id = "session_mobile_current";

        let mut current = Session::create_with_id(
            current_session_id.to_string(),
            None,
            Some("Remote resume".to_string()),
        );
        current.rename_title(Some("Renamed remote resume".to_string()));
        current.working_dir = Some("/work/mobile-safe".to_string());
        current.short_name = Some("fox".to_string());
        current.status = SessionStatus::Closed;
        current.append_stored_message(StoredMessage {
            id: "large-message".to_string(),
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "transcript content must not appear on the discovery wire".repeat(100),
                cache_control: None,
            }],
            display_role: None,
            timestamp: Some(Utc::now()),
            tool_duration_ms: None,
            token_usage: None,
        });
        current.save().expect("persist current session");
        crate::storage::register_active_pid(current_session_id, std::process::id());

        let mut debug = Session::create_with_id(
            "session_mobile_debug".to_string(),
            None,
            Some("hidden debug".to_string()),
        );
        debug.set_debug(true);
        debug.save().expect("persist debug session");

        let sessions: SessionAgents = Arc::new(RwLock::new(HashMap::new()));
        let global_session_id = Arc::new(RwLock::new(current_session_id.to_string()));
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let (client_event_tx, mut client_event_rx) = mpsc::unbounded_channel();
        handle_list_sessions_request(
            73,
            &client_event_tx,
            &sessions,
            &global_session_id,
            &swarm_members,
        )
        .await
        .expect("handle list sessions");

        let event = client_event_rx.recv().await.expect("session list event");
        let encoded = serde_json::to_string(&event).expect("encode session list");
        assert!(!encoded.contains("transcript content must not appear"));
        let crate::protocol::ServerEvent::SessionList { id, sessions } = event else {
            panic!("expected session list response");
        };
        assert_eq!(id, 73);
        assert_eq!(sessions.len(), 1);
        let entry = &sessions[0];
        assert_eq!(entry.session_id, current_session_id);
        assert_eq!(entry.title.as_deref(), Some("Renamed remote resume"));
        assert_eq!(entry.friendly_name.as_deref(), Some("fox"));
        assert_eq!(entry.working_dir.as_deref(), Some("/work/mobile-safe"));
        assert_eq!(entry.status, "closed");
        assert!(entry.is_current);
        assert!(entry.is_live);
        assert!(DateTime::parse_from_rfc3339(&entry.updated_at).is_ok());
    }
}
