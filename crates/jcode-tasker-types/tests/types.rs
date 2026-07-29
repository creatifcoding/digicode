use std::str::FromStr;

use chrono::Utc;
use jcode_tasker_types::*;

#[test]
fn typed_ids_are_prefixed_uuid_v7_and_round_trip() {
    let project_id = ProjectId::new();
    let feature_id = FeatureId::new();
    let task_id = TaskId::new();

    assert!(project_id.to_string().starts_with("proj_"));
    assert!(feature_id.to_string().starts_with("feat_"));
    assert!(task_id.to_string().starts_with("task_"));
    assert_eq!(project_id.as_uuid().get_version_num(), 7);
    assert_eq!(
        ProjectId::from_str(&project_id.to_string()).unwrap(),
        project_id
    );

    let json = serde_json::to_string(&task_id).unwrap();
    assert_eq!(serde_json::from_str::<TaskId>(&json).unwrap(), task_id);
}

#[test]
fn typed_ids_reject_wrong_prefixes_and_invalid_uuids() {
    let task_id = TaskId::new();
    let wrong_prefix = ProjectId::from_str(&task_id.to_string()).unwrap_err();
    assert_eq!(wrong_prefix.code(), "invalid_id");
    assert!(ProjectId::from_str("proj_not-a-uuid").is_err());
}

#[test]
fn aliases_have_stable_human_formats() {
    assert_eq!(FeatureAlias(12).to_string(), "#F12");
    assert_eq!(TaskAlias(184).to_string(), "#184");
}

#[test]
fn feature_transition_table_is_explicit() {
    let states = [
        FeatureState::Open,
        FeatureState::Active,
        FeatureState::Closed,
        FeatureState::Archived,
    ];
    let allowed = [
        [true, true, true, true],
        [true, true, true, true],
        [true, false, true, true],
        [true, false, false, true],
    ];

    for (from_index, from) in states.into_iter().enumerate() {
        for (to_index, to) in states.into_iter().enumerate() {
            assert_eq!(
                from.can_transition_to(to),
                allowed[from_index][to_index],
                "{from:?} -> {to:?}"
            );
        }
    }

    let error = FeatureState::Archived
        .transition_to(FeatureState::Closed)
        .unwrap_err();
    assert_eq!(error.code(), "invalid_transition");
}

#[test]
fn task_transition_table_is_explicit() {
    let states = [
        TaskState::Todo,
        TaskState::InProgress,
        TaskState::Blocked,
        TaskState::Done,
        TaskState::Cancelled,
    ];
    let allowed = [
        [true, true, true, false, true],
        [false, true, true, true, true],
        [true, true, true, false, true],
        [true, false, false, true, false],
        [true, false, false, false, true],
    ];

    for (from_index, from) in states.into_iter().enumerate() {
        for (to_index, to) in states.into_iter().enumerate() {
            assert_eq!(
                from.can_transition_to(to),
                allowed[from_index][to_index],
                "{from:?} -> {to:?}"
            );
        }
    }

    assert!(TaskState::Done.is_terminal());
    assert!(TaskState::Cancelled.is_terminal());
    assert!(TaskState::Done.satisfies_dependency());
    assert!(!TaskState::Cancelled.satisfies_dependency());
    assert!(TaskState::Todo.permits_execution());
}

#[test]
fn commands_and_events_use_stable_tagged_json() {
    let command = TaskerCommand::CreateProject(CreateProject {
        id: None,
        name: "Native tasker".to_string(),
        canonical_root: Some("/repo".to_string()),
    });
    let value = serde_json::to_value(&command).unwrap();
    assert_eq!(value["action"], "create_project");
    assert_eq!(
        serde_json::from_value::<TaskerCommand>(value).unwrap(),
        command
    );

    let event = ProjectEvent {
        id: OutboxEventId::new(),
        project_id: ProjectId::new(),
        revision: ProjectRevision(3),
        change: ChangeSummary {
            kind: ChangeKind::TaskCreated,
            feature_id: Some(FeatureId::new()),
            task_id: Some(TaskId::new()),
            description: "created task".to_string(),
        },
        created_at: Utc::now(),
    };
    let event_json = serde_json::to_string(&event).unwrap();
    assert_eq!(
        serde_json::from_str::<ProjectEvent>(&event_json).unwrap(),
        event
    );
}

#[test]
fn errors_are_machine_readable_and_serializable() {
    let error = TaskerError::RevisionConflict {
        expected: ProjectRevision(2),
        actual: ProjectRevision(4),
    };
    assert_eq!(error.code(), "revision_conflict");
    assert!(error.to_string().contains("expected 2"));

    let json = serde_json::to_string(&error).unwrap();
    assert_eq!(serde_json::from_str::<TaskerError>(&json).unwrap(), error);
}

#[test]
fn snapshots_provide_bounded_lookup_helpers() {
    let project_id = ProjectId::new();
    let feature_id = FeatureId::new();
    let task_id = TaskId::new();
    let now = Utc::now();
    let task = Task {
        id: task_id,
        project_id,
        feature_id,
        alias: TaskAlias(1),
        title: "Persist state".to_string(),
        description: String::new(),
        state: TaskState::Todo,
        priority: TaskPriority::Normal,
        rank: 0,
        created_at: now,
        updated_at: now,
    };
    let ready = ReadyTask {
        task: task.clone(),
        explanation: ReadinessExplanation {
            task_id,
            ready: true,
            satisfied_dependencies: Vec::new(),
            unsatisfied_dependencies: Vec::new(),
            restrictions: Vec::new(),
            revision: ProjectRevision(1),
            policy_version: INITIAL_READINESS_POLICY_VERSION,
        },
    };
    let snapshot = ProjectSnapshot {
        project: Project {
            id: project_id,
            name: "Tasker".to_string(),
            canonical_root: None,
            revision: ProjectRevision(1),
            created_at: now,
            updated_at: now,
        },
        revision: ProjectRevision(1),
        features: Vec::new(),
        tasks: vec![task],
        ready_tasks: vec![ready],
        policy_version: INITIAL_READINESS_POLICY_VERSION,
    };

    assert_eq!(snapshot.task(task_id).unwrap().alias, TaskAlias(1));
    assert!(snapshot.ready_task(task_id).unwrap().explanation.ready);
}
