use jcode_tasker_store::{StoreError, TaskerStore};
use jcode_tasker_types::{
    AddFeatureDependency, AddTaskDependency, CreateFeature, CreateProject, CreateTask,
    FeatureState, ProjectRevision, SetFeatureState, SetTaskState, TaskPriority, TaskState,
    TaskerError,
};

async fn seeded_store() -> (
    TaskerStore,
    jcode_tasker_types::Project,
    jcode_tasker_types::Feature,
) {
    let store = TaskerStore::open_in_memory().await.unwrap();
    let project = store
        .create_project(CreateProject {
            id: None,
            name: "Tasker".into(),
            canonical_root: Some("/workspace".into()),
        })
        .await
        .unwrap()
        .value;
    let feature = store
        .create_feature(CreateFeature {
            project_id: project.id,
            id: None,
            parent_id: None,
            title: "Foundation".into(),
            description: String::new(),
            expected_revision: Some(project.revision),
        })
        .await
        .unwrap()
        .value;
    (store, project, feature)
}

fn task_command(
    project_id: jcode_tasker_types::ProjectId,
    feature_id: jcode_tasker_types::FeatureId,
    title: &str,
    priority: TaskPriority,
    rank: i64,
    revision: ProjectRevision,
) -> CreateTask {
    CreateTask {
        project_id,
        feature_id,
        id: None,
        title: title.into(),
        description: String::new(),
        priority,
        rank,
        expected_revision: Some(revision),
    }
}

#[tokio::test]
async fn configures_sqlite_and_migrates_once() {
    let store = TaskerStore::open_in_memory().await.unwrap();
    let state = store.connection_state().await.unwrap();
    assert!(state.foreign_keys);
    assert_eq!(state.schema_version, 1);
    assert!(state.busy_timeout_ms >= 5_000);
    assert!(matches!(state.journal_mode.as_str(), "wal" | "memory"));
}

#[tokio::test]
async fn file_store_uses_wal_and_foreign_keys() {
    let directory = tempfile::tempdir().unwrap();
    let store = TaskerStore::open(directory.path().join("tasker.db"))
        .await
        .unwrap();
    let state = store.connection_state().await.unwrap();
    assert_eq!(state.journal_mode, "wal");
    assert!(state.foreign_keys);
    assert_eq!(state.synchronous, 1);
}

#[tokio::test]
async fn revisions_aliases_and_outbox_are_monotonic() {
    let (store, project, feature) = seeded_store().await;
    assert_eq!(project.revision, ProjectRevision(1));
    assert_eq!(feature.alias.0, 1);

    let task = store
        .create_task(task_command(
            project.id,
            feature.id,
            "First",
            TaskPriority::Normal,
            0,
            ProjectRevision(2),
        ))
        .await
        .unwrap();
    assert_eq!(task.revision, ProjectRevision(3));
    assert_eq!(task.value.alias.0, 1);

    let second = store
        .create_task(task_command(
            project.id,
            feature.id,
            "Second",
            TaskPriority::High,
            0,
            ProjectRevision(3),
        ))
        .await
        .unwrap();
    assert_eq!(second.revision, ProjectRevision(4));
    assert_eq!(second.value.alias.0, 2);

    let events = store.pending_outbox(Some(project.id), 100).await.unwrap();
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].event.revision, ProjectRevision(1));
    assert_eq!(events[3].event.revision, ProjectRevision(4));
    assert!(
        store
            .mark_outbox_dispatched(events[0].event.id)
            .await
            .unwrap()
    );
    assert!(
        !store
            .mark_outbox_dispatched(events[0].event.id)
            .await
            .unwrap()
    );
    assert_eq!(
        store
            .pending_outbox(Some(project.id), 100)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn stale_revision_rolls_back_without_consuming_alias_or_revision() {
    let (store, project, feature) = seeded_store().await;
    let error = store
        .create_task(task_command(
            project.id,
            feature.id,
            "Stale",
            TaskPriority::Normal,
            0,
            ProjectRevision(1),
        ))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        StoreError::Domain(TaskerError::RevisionConflict {
            expected: ProjectRevision(1),
            actual: ProjectRevision(2)
        })
    ));

    let task = store
        .create_task(task_command(
            project.id,
            feature.id,
            "Fresh",
            TaskPriority::Normal,
            0,
            ProjectRevision(2),
        ))
        .await
        .unwrap();
    assert_eq!(task.value.alias.0, 1);
    assert_eq!(task.revision, ProjectRevision(3));
}

#[tokio::test]
async fn readiness_is_dependency_aware_and_deterministically_ranked() {
    let (store, project, feature) = seeded_store().await;
    let low = store
        .create_task(task_command(
            project.id,
            feature.id,
            "Low",
            TaskPriority::Low,
            10,
            ProjectRevision(2),
        ))
        .await
        .unwrap();
    let critical = store
        .create_task(task_command(
            project.id,
            feature.id,
            "Critical",
            TaskPriority::Critical,
            20,
            ProjectRevision(3),
        ))
        .await
        .unwrap();
    let dependent = store
        .create_task(task_command(
            project.id,
            feature.id,
            "Dependent",
            TaskPriority::Critical,
            0,
            ProjectRevision(4),
        ))
        .await
        .unwrap();
    store
        .add_task_dependency(AddTaskDependency {
            project_id: project.id,
            task_id: dependent.value.id,
            depends_on_task_id: low.value.id,
            expected_revision: Some(ProjectRevision(5)),
        })
        .await
        .unwrap();

    let snapshot = store.snapshot(project.id).await.unwrap();
    let ready_ids: Vec<_> = snapshot
        .ready_tasks
        .iter()
        .map(|ready| ready.task.id)
        .collect();
    assert_eq!(ready_ids, vec![critical.value.id, low.value.id]);

    store
        .set_task_state(SetTaskState {
            project_id: project.id,
            task_id: low.value.id,
            state: TaskState::InProgress,
            expected_revision: Some(snapshot.revision),
        })
        .await
        .unwrap();
    store
        .set_task_state(SetTaskState {
            project_id: project.id,
            task_id: low.value.id,
            state: TaskState::Done,
            expected_revision: Some(ProjectRevision(snapshot.revision.0 + 1)),
        })
        .await
        .unwrap();

    let snapshot = store.snapshot(project.id).await.unwrap();
    let ready_ids: Vec<_> = snapshot
        .ready_tasks
        .iter()
        .map(|ready| ready.task.id)
        .collect();
    assert_eq!(ready_ids, vec![dependent.value.id, critical.value.id]);
    let explanation = &snapshot.ready_task(dependent.value.id).unwrap().explanation;
    assert_eq!(explanation.satisfied_dependencies, vec![low.value.id]);
}

#[tokio::test]
async fn dependency_cycles_are_rejected_without_partial_state() {
    let (store, project, feature) = seeded_store().await;
    let first = store
        .create_task(task_command(
            project.id,
            feature.id,
            "First",
            TaskPriority::Normal,
            0,
            ProjectRevision(2),
        ))
        .await
        .unwrap();
    let second = store
        .create_task(task_command(
            project.id,
            feature.id,
            "Second",
            TaskPriority::Normal,
            0,
            ProjectRevision(3),
        ))
        .await
        .unwrap();
    store
        .add_task_dependency(AddTaskDependency {
            project_id: project.id,
            task_id: second.value.id,
            depends_on_task_id: first.value.id,
            expected_revision: Some(ProjectRevision(4)),
        })
        .await
        .unwrap();

    let error = store
        .add_task_dependency(AddTaskDependency {
            project_id: project.id,
            task_id: first.value.id,
            depends_on_task_id: second.value.id,
            expected_revision: Some(ProjectRevision(5)),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        StoreError::Domain(TaskerError::DependencyCycle { .. })
    ));
    assert_eq!(
        store.snapshot(project.id).await.unwrap().revision,
        ProjectRevision(5)
    );
}

#[tokio::test]
async fn feature_cycles_and_closed_ancestors_are_enforced() {
    let (store, project, parent) = seeded_store().await;
    let child = store
        .create_feature(CreateFeature {
            project_id: project.id,
            id: None,
            parent_id: Some(parent.id),
            title: "Child".into(),
            description: String::new(),
            expected_revision: Some(ProjectRevision(2)),
        })
        .await
        .unwrap();
    store
        .add_feature_dependency(AddFeatureDependency {
            project_id: project.id,
            feature_id: child.value.id,
            depends_on_feature_id: parent.id,
            expected_revision: Some(ProjectRevision(3)),
        })
        .await
        .unwrap();
    let cycle = store
        .add_feature_dependency(AddFeatureDependency {
            project_id: project.id,
            feature_id: parent.id,
            depends_on_feature_id: child.value.id,
            expected_revision: Some(ProjectRevision(4)),
        })
        .await
        .unwrap_err();
    assert!(matches!(
        cycle,
        StoreError::Domain(TaskerError::DependencyCycle { .. })
    ));

    let task = store
        .create_task(task_command(
            project.id,
            child.value.id,
            "Nested task",
            TaskPriority::Normal,
            0,
            ProjectRevision(4),
        ))
        .await
        .unwrap();
    store
        .set_feature_state(SetFeatureState {
            project_id: project.id,
            feature_id: parent.id,
            state: FeatureState::Closed,
            expected_revision: Some(ProjectRevision(5)),
        })
        .await
        .unwrap();
    let readiness = store
        .task_readiness(project.id, task.value.id)
        .await
        .unwrap();
    assert!(!readiness.ready);
    assert!(
        readiness
            .restrictions
            .iter()
            .any(|restriction| restriction.contains(&parent.id.to_string()))
    );
}

#[tokio::test]
async fn file_backed_store_survives_reopen_with_pending_outbox() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("tasker.db");
    let project_id;
    {
        let store = TaskerStore::open(&path).await.unwrap();
        let project = store
            .create_project(CreateProject {
                id: None,
                name: "Persistent".into(),
                canonical_root: None,
            })
            .await
            .unwrap()
            .value;
        project_id = project.id;
    }

    let reopened = TaskerStore::open(&path).await.unwrap();
    assert_eq!(
        reopened.get_project(project_id).await.unwrap().name,
        "Persistent"
    );
    let events = reopened.pending_outbox(Some(project_id), 10).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event.revision, ProjectRevision(1));
}
