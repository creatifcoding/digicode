use super::*;

#[test]
fn page_scope_keeps_owner_identity_and_scope_kind_separate() {
    let session = PageScope::session("ses-1");
    let project = PageScope::project("/workspace/project");
    let global = PageScope::global("profile-1");

    assert_eq!(session.kind(), SidePanelScope::Session);
    assert_eq!(session.owner_id(), "ses-1");
    assert_eq!(session.owner_key(), "session:ses-1");
    assert_eq!(project.kind(), SidePanelScope::Project);
    assert_eq!(project.owner_id(), "/workspace/project");
    assert_eq!(global.kind(), SidePanelScope::Global);
    assert_eq!(global.owner_id(), "profile-1");
    assert!(project.is_shared());
    assert!(global.is_shared());
    assert!(!session.is_shared());
}

#[test]
fn ephemeral_pages_are_session_owned_only() {
    let session = PageScope::session("ses-1");
    let project = PageScope::project("/workspace/project");
    let global = PageScope::global("profile-1");

    assert!(session.allows_source(SidePanelPageSource::Ephemeral));
    assert!(!project.allows_source(SidePanelPageSource::Ephemeral));
    assert!(!global.allows_source(SidePanelPageSource::Ephemeral));
    assert!(project.allows_source(SidePanelPageSource::Managed));
    assert!(global.allows_source(SidePanelPageSource::LinkedFile));

    let error =
        PageDocumentRecord::new("graph", project, SidePanelPageSource::Ephemeral, "Graph", 1)
            .expect_err("shared scopes must reject ephemeral documents");
    assert!(error.to_string().contains("session-owned"));
}

#[test]
fn document_identity_and_reference_view_state_are_separate() {
    let document = PageDocumentRecord::new(
        "architecture",
        PageScope::project("/workspace/project"),
        SidePanelPageSource::Managed,
        "Architecture",
        7,
    )
    .expect("valid project document");

    let mut first = WorkspacePageReference::new(document.scoped_id());
    let mut second = WorkspacePageReference::new(document.scoped_id());
    first.pinned = true;
    first.view.scroll_y = 12;
    first.view.graph_zoom_percent = 140;
    first.view.selected_element_id = Some("node-a".to_string());
    first.mark_viewed(100);
    second.view.scroll_y = 88;
    second.view.graph_pan_x = -4;
    second.mark_viewed(200);

    assert_eq!(first.page, second.page);
    assert_ne!(first.view, second.view);
    assert_ne!(first.last_viewed_at_ms, second.last_viewed_at_ms);
    assert!(first.pinned);
    assert!(!second.pinned);
    assert_eq!(document.scope.owner_id(), "/workspace/project");
    assert_eq!(document.revision, 7);
}

#[test]
fn view_state_defaults_are_portable_and_workspace_is_session_owned() {
    let view = PageViewState::default();
    assert_eq!(view.scroll_x, 0);
    assert_eq!(view.scroll_y, 0);
    assert_eq!(view.graph_zoom_percent, 100);
    assert!(view.collapsed_sections.is_empty());
    assert!(view.search_query.is_empty());
    assert!(view.selected_element_id.is_none());
    assert!(view.focused_element_id.is_none());

    let mut workspace = SidePanelWorkspaceState::new("ses-1");
    let page = ScopedPageId::new(PageScope::global("profile-1"), "manual");
    workspace.focused_page = Some(page.clone());
    workspace.pages.push(WorkspacePageReference::new(page));

    assert_eq!(workspace.session_id.as_str(), "ses-1");
    assert_eq!(workspace.pages[0].page.scope.kind(), SidePanelScope::Global);
    assert_eq!(
        workspace.focused_page,
        Some(workspace.pages[0].page.clone())
    );
}
