//! Jcode's in-process Linux computer-use adapter.
//!
//! The backend lives in the source-owned `jcode-computer-use-linux` crate. This
//! adapter deliberately calls it as a library. No MCP child process, downloaded
//! executable, or external checkout is involved at runtime.

use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context, Result};
use async_trait::async_trait;
use jcode_computer_use_linux::server::ComputerUseLinux;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct LinuxComputerTool;

impl LinuxComputerTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
struct LinuxComputerInput {
    action: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    dry_run: Option<bool>,
}

fn is_mutating(action: &str) -> bool {
    matches!(
        action,
        "setup_accessibility"
            | "setup_window_targeting"
            | "activate_window"
            | "click"
            | "perform_action"
            | "set_value"
            | "scroll"
            | "drag"
            | "press_key"
            | "key"
            | "type_text"
            | "type"
            | "move_window"
            | "resize_window"
    )
}

fn discover() -> ToolOutput {
    ToolOutput::new(
        "linux_computer_use actions:\n\n\
         Observe: doctor, check_readiness, screenshot, list_apps, list_windows, focused_window, get_app_state, ui.\n\
         Input: click, scroll, drag, press_key/key, type_text/type, perform_action, set_value.\n\
         Windows/setup: activate_window, move_window, resize_window, setup_accessibility, setup_window_targeting.\n\n\
         Put action-specific fields in `params`. Mutating actions support `dry_run=true`. The backend executes in-process from Jcode-owned source.",
    )
}

#[async_trait]
impl Tool for LinuxComputerTool {
    fn name(&self) -> &str {
        "linux_computer_use"
    }

    fn description(&self) -> &str {
        "Inspect and control the Linux desktop through Jcode's source-owned in-process backend. Supports Niri, GNOME, KDE/KWin, Hyprland, i3, and COSMIC window discovery where available; XDG portal/GNOME screenshot capture; AT-SPI accessibility; and portal, uinput, or ydotool input backends. Begin with action='doctor'. Use action='discover' for the action list. This is the user's live desktop: prefer read-only observation, use dry_run for mutations, and never submit, delete, purchase, send, or overwrite without explicit user approval."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "description": "Action name. Start with doctor; use discover for the complete compact list."
                },
                "params": {
                    "type": "object",
                    "description": "Action-specific backend parameters, for example {x,y} for click, {text} for type_text, or {app_name_or_bundle_identifier,max_nodes,max_depth,include_screenshot} for get_app_state."
                },
                "dry_run": {
                    "type": "boolean",
                    "description": "For mutating actions, report the intended call without changing desktop state."
                }
            }
        })
    }

    async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
        let input: LinuxComputerInput =
            serde_json::from_value(input).context("invalid `linux_computer_use` tool input")?;
        let action = input.action.trim();
        if action == "discover" {
            return Ok(discover());
        }
        if input.dry_run == Some(true) && is_mutating(action) {
            return Ok(ToolOutput::new(format!(
                "[dry_run] would invoke Linux computer-use action `{action}` with params {}. No action taken.",
                input.params
            ))
            .with_metadata(json!({"dry_run": true, "action": action, "params": input.params})));
        }

        let backend = ComputerUseLinux::default();
        if action == "screenshot" {
            let capture = backend.screenshot_capture().await?;
            let data = capture
                .data_url
                .split_once(',')
                .map(|(_, data)| data)
                .unwrap_or(capture.data_url.as_str())
                .to_string();
            let metadata = serde_json::to_value(&capture)?;
            return Ok(ToolOutput::new(format!(
                "Captured Linux desktop via {} at {}x{} (coordinate space {}x{}, scale {}).",
                capture.source,
                capture.width,
                capture.height,
                capture.coordinate_width,
                capture.coordinate_height,
                capture.scale
            ))
            .with_labeled_image(capture.mime_type, data, "Linux desktop screenshot")
            .with_metadata(metadata));
        }

        let value = backend.execute_json(action, input.params).await?;
        Ok(ToolOutput::new(serde_json::to_string_pretty(&value)?).with_metadata(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jcode_tool_core::ToolExecutionMode;

    fn ctx() -> ToolContext {
        ToolContext {
            session_id: "test".into(),
            message_id: "test".into(),
            tool_call_id: "test".into(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: ToolExecutionMode::Direct,
        }
    }

    #[test]
    fn mutating_actions_are_classified() {
        assert!(is_mutating("click"));
        assert!(is_mutating("type_text"));
        assert!(!is_mutating("doctor"));
        assert!(!is_mutating("screenshot"));
    }

    #[test]
    fn discover_names_core_actions() {
        let output = discover().output;
        assert!(output.contains("doctor"));
        assert!(output.contains("screenshot"));
        assert!(output.contains("click"));
    }

    #[tokio::test]
    async fn mutating_action_dry_run_does_not_reach_backend() {
        let output = LinuxComputerTool::new()
            .execute(
                json!({
                    "action": "click",
                    "params": { "x": 12, "y": 34 },
                    "dry_run": true
                }),
                ctx(),
            )
            .await
            .expect("dry-run should not require a live input backend");

        assert!(output.output.contains("No action taken"));
        assert_eq!(
            output
                .metadata
                .as_ref()
                .and_then(|v| v["dry_run"].as_bool()),
            Some(true)
        );
        assert_eq!(
            output.metadata.as_ref().and_then(|v| v["action"].as_str()),
            Some("click")
        );
    }

    #[tokio::test]
    async fn doctor_runs_through_the_in_process_backend() {
        let output = LinuxComputerTool::new()
            .execute(json!({ "action": "doctor" }), ctx())
            .await
            .expect(
                "doctor should return a capability report even when capabilities are unavailable",
            );

        let metadata = output
            .metadata
            .expect("doctor should expose structured metadata");
        assert_eq!(metadata["platform"]["os"], "linux");
    }

    #[tokio::test]
    #[ignore = "requires a live Linux desktop session"]
    async fn live_window_discovery_runs_through_the_built_in_tool() {
        let output = LinuxComputerTool::new()
            .execute(json!({ "action": "list_windows" }), ctx())
            .await
            .expect("window discovery should use the active compositor backend");

        let metadata = output.metadata.expect("window list should be structured");
        assert!(matches!(metadata["backend"].as_str(), Some("niri" | "i3")));
        assert!(
            metadata["windows"]
                .as_array()
                .is_some_and(|windows| !windows.is_empty())
        );
    }

    #[tokio::test]
    #[ignore = "requires a live Linux desktop session"]
    async fn live_screenshot_attaches_an_image() {
        let output = LinuxComputerTool::new()
            .execute(json!({ "action": "screenshot" }), ctx())
            .await
            .expect("screenshot should use an available desktop capture backend");

        assert_eq!(output.images.len(), 1);
        assert!(!output.images[0].data.is_empty());
    }
}
