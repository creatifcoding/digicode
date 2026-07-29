CREATE TABLE projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    canonical_root TEXT,
    next_feature_alias INTEGER NOT NULL DEFAULT 1 CHECK (next_feature_alias > 0),
    next_task_alias INTEGER NOT NULL DEFAULT 1 CHECK (next_task_alias > 0),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE project_roots (
    project_id TEXT NOT NULL,
    root TEXT NOT NULL,
    is_canonical INTEGER NOT NULL DEFAULT 0 CHECK (is_canonical IN (0, 1)),
    created_at TEXT NOT NULL,
    PRIMARY KEY (project_id, root),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) STRICT;

CREATE UNIQUE INDEX one_canonical_root_per_project
    ON project_roots(project_id)
    WHERE is_canonical = 1;

CREATE TABLE project_revisions (
    project_id TEXT PRIMARY KEY NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    updated_at TEXT NOT NULL,
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) STRICT;

CREATE TABLE features (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    alias INTEGER NOT NULL CHECK (alias > 0),
    parent_id TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL CHECK (state IN ('open', 'active', 'closed', 'archived')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, id),
    UNIQUE (project_id, alias),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, parent_id) REFERENCES features(project_id, id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX features_by_parent ON features(project_id, parent_id, alias);

CREATE TABLE feature_dependencies (
    project_id TEXT NOT NULL,
    feature_id TEXT NOT NULL,
    depends_on_feature_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (project_id, feature_id, depends_on_feature_id),
    CHECK (feature_id <> depends_on_feature_id),
    FOREIGN KEY (project_id, feature_id) REFERENCES features(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, depends_on_feature_id) REFERENCES features(project_id, id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX feature_dependencies_reverse
    ON feature_dependencies(project_id, depends_on_feature_id, feature_id);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    feature_id TEXT NOT NULL,
    alias INTEGER NOT NULL CHECK (alias > 0),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT NOT NULL DEFAULT '',
    state TEXT NOT NULL CHECK (state IN ('todo', 'in_progress', 'blocked', 'done', 'cancelled')),
    priority INTEGER NOT NULL DEFAULT 1 CHECK (priority BETWEEN 0 AND 3),
    rank INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, id),
    UNIQUE (project_id, alias),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, feature_id) REFERENCES features(project_id, id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX tasks_ready_order
    ON tasks(project_id, state, priority DESC, rank, alias, id);
CREATE INDEX tasks_by_feature ON tasks(project_id, feature_id, alias);

CREATE TABLE task_dependencies (
    project_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    depends_on_task_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (project_id, task_id, depends_on_task_id),
    CHECK (task_id <> depends_on_task_id),
    FOREIGN KEY (project_id, task_id) REFERENCES tasks(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, depends_on_task_id) REFERENCES tasks(project_id, id) ON DELETE RESTRICT
) STRICT;

CREATE INDEX task_dependencies_reverse
    ON task_dependencies(project_id, depends_on_task_id, task_id);

CREATE TABLE outbox_events (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    created_at TEXT NOT NULL,
    dispatched_at TEXT,
    UNIQUE (project_id, revision),
    FOREIGN KEY (project_id) REFERENCES projects(id) ON DELETE CASCADE
) STRICT;

CREATE INDEX outbox_pending
    ON outbox_events(dispatched_at, project_id, revision)
    WHERE dispatched_at IS NULL;
