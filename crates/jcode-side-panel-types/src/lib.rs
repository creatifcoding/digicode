//! Renderer-neutral data contracts for the Alt+M side panel.
//!
//! The current [`SidePanelSnapshot`] is a session-scoped wire payload used by
//! the existing tool, server, and TUI. The ownership and workspace types in
//! this module are the next-layer contract for the durable workspace:
//!
//! - [`PageDocumentRecord`] describes one owned document and its source.
//! - [`WorkspacePageReference`] describes one workspace's local relationship
//!   with that document.
//! - [`PageViewState`] contains only portable per-page view state. It does not
//!   depend on Ratatui, Mermaid, Tasker, swarm, or another domain adapter.
//!
//! A document is stored once at its [`PageScope`]. Session workspaces may keep
//! independent references to Project and Global documents, so reading state,
//! selection, graph transforms, and local ordering never overwrite the shared
//! document record or another session's view.

use serde::{Deserialize, Serialize};
use std::path::Path;

macro_rules! define_string_id {
    ($name:ident, $doc:literal) => {
        #[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
        #[serde(transparent)]
        #[doc = $doc]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn into_inner(self) -> String {
                self.0
            }

            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }

        impl From<&String> for $name {
            fn from(value: &String) -> Self {
                Self(value.clone())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

define_string_id!(
    PageId,
    "Stable identity for a side-panel document within its owning scope."
);

define_string_id!(
    SessionId,
    "Stable Jcode conversation identity used by Session scope."
);

define_string_id!(UserProfileId, "Stable user identity used by Global scope.");

define_string_id!(
    CanonicalProjectRoot,
    "Canonical project-root identity used by Project scope. `new` and `From` accept a value that the caller has already canonicalized. Use `from_path` when the identity must be established from a working directory."
);

impl CanonicalProjectRoot {
    /// Canonicalize a project path before using it as a Project owner.
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let canonical = std::fs::canonicalize(path)?;
        Ok(Self::new(canonical.to_string_lossy().into_owned()))
    }
}

/// The three ownership kinds used by the current and proposed side-panel
/// workspace. This is the scope kind only; [`PageScope`] carries its owner
/// identity as well.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
#[serde(rename_all = "snake_case")]
pub enum SidePanelScope {
    #[default]
    Session,
    Project,
    Global,
}

impl SidePanelScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Project => "project",
            Self::Global => "global",
        }
    }

    pub fn is_shared(self) -> bool {
        matches!(self, Self::Project | Self::Global)
    }
}

/// Identifies the owner of a page document, not the workspace currently
/// viewing it.
///
/// Project identity is a canonical root, never an arbitrary spelling of the
/// current working directory. A Session owner is the only legal owner for an
/// [`SidePanelPageSource::Ephemeral`] document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PageScope {
    Session(SessionId),
    Project(CanonicalProjectRoot),
    Global(UserProfileId),
}

impl PageScope {
    pub fn session(session_id: impl Into<SessionId>) -> Self {
        Self::Session(session_id.into())
    }

    pub fn project(project_root: impl Into<CanonicalProjectRoot>) -> Self {
        Self::Project(project_root.into())
    }

    pub fn global(profile_id: impl Into<UserProfileId>) -> Self {
        Self::Global(profile_id.into())
    }

    pub fn kind(&self) -> SidePanelScope {
        match self {
            Self::Session(_) => SidePanelScope::Session,
            Self::Project(_) => SidePanelScope::Project,
            Self::Global(_) => SidePanelScope::Global,
        }
    }

    pub fn as_str(&self) -> &'static str {
        self.kind().as_str()
    }

    pub fn owner_id(&self) -> &str {
        match self {
            Self::Session(id) => id.as_str(),
            Self::Project(id) => id.as_str(),
            Self::Global(id) => id.as_str(),
        }
    }

    /// Return a stable, human-readable owner key for storage adapters.
    /// Storage is deliberately outside this crate.
    pub fn owner_key(&self) -> String {
        format!("{}:{}", self.as_str(), self.owner_id())
    }

    pub fn is_session(&self) -> bool {
        matches!(self, Self::Session(_))
    }

    pub fn is_shared(&self) -> bool {
        self.kind().is_shared()
    }

    /// Ephemeral pages are runtime-only and cannot be promoted into a shared
    /// Project or Global catalog.
    pub fn allows_source(&self, source: SidePanelPageSource) -> bool {
        !matches!(source, SidePanelPageSource::Ephemeral) || self.is_session()
    }
}

/// Failure raised when a document violates its scope ownership invariant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageOwnershipError {
    EphemeralRequiresSession { scope: PageScope },
}

impl std::fmt::Display for PageOwnershipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EphemeralRequiresSession { scope } => write!(
                f,
                "ephemeral side-panel pages must be session-owned, not {}",
                scope.as_str()
            ),
        }
    }
}

impl std::error::Error for PageOwnershipError {}

/// A page identity qualified by the scope that owns its document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ScopedPageId {
    pub scope: PageScope,
    pub page_id: PageId,
}

impl ScopedPageId {
    pub fn new(scope: PageScope, page_id: impl Into<PageId>) -> Self {
        Self {
            scope,
            page_id: page_id.into(),
        }
    }
}

/// Document metadata owned by one Session, Project, or Global scope.
///
/// Content remains in the existing page payload/storage pipeline. This record
/// intentionally contains no renderer or domain-specific state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageDocumentRecord {
    pub id: PageId,
    pub scope: PageScope,
    pub source: SidePanelPageSource,
    pub title: String,
    pub revision: u64,
}

impl PageDocumentRecord {
    pub fn new(
        id: impl Into<PageId>,
        scope: PageScope,
        source: SidePanelPageSource,
        title: impl Into<String>,
        revision: u64,
    ) -> Result<Self, PageOwnershipError> {
        let record = Self {
            id: id.into(),
            scope,
            source,
            title: title.into(),
            revision,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> Result<(), PageOwnershipError> {
        if self.scope.allows_source(self.source) {
            Ok(())
        } else {
            Err(PageOwnershipError::EphemeralRequiresSession {
                scope: self.scope.clone(),
            })
        }
    }

    pub fn scoped_id(&self) -> ScopedPageId {
        ScopedPageId::new(self.scope.clone(), self.id.clone())
    }
}

/// Portable view state kept with a workspace reference rather than the shared
/// document. Integer graph transforms keep this contract serializable and
/// renderer-neutral; a renderer may map them to its own coordinate system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PageViewState {
    #[serde(default)]
    pub scroll_x: u32,
    #[serde(default)]
    pub scroll_y: u32,
    #[serde(default = "default_graph_zoom_percent")]
    pub graph_zoom_percent: u16,
    #[serde(default)]
    pub graph_pan_x: i32,
    #[serde(default)]
    pub graph_pan_y: i32,
    #[serde(default)]
    pub collapsed_sections: Vec<String>,
    #[serde(default)]
    pub search_query: String,
    #[serde(default)]
    pub search_match_index: Option<u32>,
    #[serde(default)]
    pub selected_element_id: Option<String>,
    #[serde(default)]
    pub focused_element_id: Option<String>,
}

impl Default for PageViewState {
    fn default() -> Self {
        Self {
            scroll_x: 0,
            scroll_y: 0,
            graph_zoom_percent: default_graph_zoom_percent(),
            graph_pan_x: 0,
            graph_pan_y: 0,
            collapsed_sections: Vec::new(),
            search_query: String::new(),
            search_match_index: None,
            selected_element_id: None,
            focused_element_id: None,
        }
    }
}

fn default_graph_zoom_percent() -> u16 {
    100
}

/// A local workspace reference to a shared or session-owned document.
///
/// The reference owns pinning, recency, local ordering, and per-page view
/// state. Closing a reference must not imply deleting its document.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkspacePageReference {
    pub page: ScopedPageId,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub local_order: Option<u32>,
    #[serde(default)]
    pub last_viewed_at_ms: u64,
    #[serde(default)]
    pub view: PageViewState,
}

impl WorkspacePageReference {
    pub fn new(page: ScopedPageId) -> Self {
        Self {
            page,
            pinned: false,
            local_order: None,
            last_viewed_at_ms: 0,
            view: PageViewState::default(),
        }
    }

    pub fn mark_viewed(&mut self, timestamp_ms: u64) {
        self.last_viewed_at_ms = timestamp_ms;
    }
}

/// Session-owned workspace state. The referenced documents may be Session,
/// Project, or Global pages, but all pin/order/view/focus state in this record
/// belongs to this session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SidePanelWorkspaceState {
    pub session_id: SessionId,
    #[serde(default)]
    pub focused_page: Option<ScopedPageId>,
    #[serde(default)]
    pub pages: Vec<WorkspacePageReference>,
}

impl SidePanelWorkspaceState {
    pub fn new(session_id: impl Into<SessionId>) -> Self {
        Self {
            session_id: session_id.into(),
            focused_page: None,
            pages: Vec::new(),
        }
    }
}

/// Short aliases used by future side-panel adapters while the existing
/// `SidePanel*` payload names remain wire-compatible.
pub type PageSource = SidePanelPageSource;
pub type SidePanelPageScope = PageScope;
pub type SidePanelPageDocumentRecord = PageDocumentRecord;
pub type SidePanelPageViewState = PageViewState;
pub type SidePanelPageReference = WorkspacePageReference;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SidePanelPageFormat {
    #[default]
    Markdown,
}

impl SidePanelPageFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SidePanelPageSource {
    #[default]
    Managed,
    LinkedFile,
    Ephemeral,
}

impl SidePanelPageSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::LinkedFile => "linked_file",
            Self::Ephemeral => "ephemeral",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedSidePanelState {
    #[serde(default)]
    pub focused_page_id: Option<String>,
    #[serde(default)]
    pub pages: Vec<PersistedSidePanelPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSidePanelPage {
    pub id: String,
    pub title: String,
    pub file_path: String,
    #[serde(default)]
    pub format: SidePanelPageFormat,
    #[serde(default)]
    pub source: SidePanelPageSource,
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SidePanelPage {
    pub id: String,
    pub title: String,
    pub file_path: String,
    #[serde(default)]
    pub format: SidePanelPageFormat,
    #[serde(default)]
    pub source: SidePanelPageSource,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SidePanelSnapshot {
    #[serde(default)]
    pub focused_page_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<SidePanelPage>,
}

impl SidePanelSnapshot {
    pub fn has_pages(&self) -> bool {
        !self.pages.is_empty()
    }

    pub fn focused_page(&self) -> Option<&SidePanelPage> {
        let focused_id = self.focused_page_id.as_deref()?;
        self.pages.iter().find(|page| page.id == focused_id)
    }
}

pub fn snapshot_is_empty(snapshot: &SidePanelSnapshot) -> bool {
    !snapshot.has_pages()
}

#[cfg(test)]
mod tests;
