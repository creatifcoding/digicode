//! Privacy-preserving local evidence for the native MetaTool.
//!
//! This module deliberately owns the observability and maintenance-reporting
//! surface instead of growing `tool/metatool.rs` into a second persistence
//! layer. The persisted ontology is intentionally small:
//!
//! - [`TraceRecord`] is execution evidence for every MetaTool call.
//! - [`DiagnosticDump`] is failure-only bounded evidence attached to a trace.
//! - [`Finding`] is an unconfirmed maintenance observation.
//! - [`TriageDisposition`] classifies a finding without filing anything.
//! - [`IssueProposal`] is an explicit local proposal, never an external issue.
//!
//! No raw MetaTool source, inputs, prompts, filenames, code, output, stacks, or
//! secrets are written to traces or sent through telemetry. Finding text has a
//! separate local-only path and is secret-redacted and bounded before storage.

use std::{
    cmp::Reverse,
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const TRACE_SCHEMA_VERSION: u8 = 1;
const TRACE_FILE: &str = "traces.jsonl";
const FINDINGS_FILE: &str = "findings.json";
const MAX_TRACE_RECORDS: usize = 512;
const MAX_TRACE_BYTES: usize = 512 * 1024;
const MAX_FINDINGS: usize = 256;
const MAX_FINDING_TEXT_CHARS: usize = 2_000;
const RETENTION_MS: u64 = 30 * 24 * 60 * 60 * 1_000;

static OBSERVABILITY_LOCK: Mutex<()> = Mutex::new(());
static REDACTION_PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

/// A maintenance observation is not an issue. It starts unconfirmed and must
/// be explicitly triaged before an issue proposal can be recorded.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TriageDisposition {
    #[default]
    Unconfirmed,
    NeedsReproduction,
    SuspectedBug,
    ConfirmedBug,
    NotABug,
    Duplicate,
    AcceptedMaintenance,
}

impl TriageDisposition {
    fn allows_issue_proposal(&self) -> bool {
        matches!(self, Self::SuspectedBug | Self::ConfirmedBug)
    }
}

/// Failure-only evidence. It contains shape, size, and digest metadata, not
/// the failed call's source, input, output, or stack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticDump {
    pub error_class: String,
    pub message_bytes: usize,
    pub message_hash: String,
}

/// Durable execution evidence for one MetaTool invocation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TraceRecord {
    pub schema_version: u8,
    pub trace_id: String,
    pub recorded_at_ms: u64,
    pub action: String,
    pub outcome: String,
    pub duration_ms: u64,
    pub input_shape: String,
    pub input_key_count: usize,
    pub input_bytes: usize,
    pub input_hash: String,
    pub source_present: bool,
    pub source_bytes: usize,
    pub source_hash: Option<String>,
    pub inputs_present: bool,
    pub inputs_bytes: usize,
    pub inputs_hash: Option<String>,
    pub output_bytes: usize,
    pub output_hash: Option<String>,
    pub metadata_bytes: usize,
    pub metadata_hash: Option<String>,
    pub image_count: usize,
    pub context_hash: String,
    pub diagnostic_dump: Option<DiagnosticDump>,
}

/// A durable, local-only maintenance observation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub context_hash: String,
    pub title: String,
    pub summary: String,
    pub disposition: TriageDisposition,
    pub triage_note: Option<String>,
    pub triaged_at_ms: Option<u64>,
    pub issue_proposals: Vec<IssueProposal>,
}

/// An explicit proposal recorded locally for later human review. The host
/// never sends this to GitHub, an issue tracker, or any other external system.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct IssueProposal {
    pub id: String,
    pub finding_id: String,
    pub created_at_ms: u64,
    pub title: String,
    pub summary: String,
    pub status: String,
    pub external_filing: bool,
}

/// The small amount of call context permitted to local observability.
#[derive(Debug, Clone, Copy)]
pub struct InvocationContext<'a> {
    pub session_id: &'a str,
    pub message_id: &'a str,
    pub tool_call_id: &'a str,
    pub working_dir: Option<&'a Path>,
}

/// The input required by a direct agent finding report.
#[derive(Debug, Clone, Copy)]
pub struct FindingContext<'a> {
    pub session_id: &'a str,
    pub working_dir: Option<&'a Path>,
}

#[derive(Debug, Clone)]
pub enum FindingAction<'a> {
    Report {
        title: &'a str,
        summary: &'a str,
    },
    List {
        limit: Option<usize>,
    },
    Show {
        finding_id: &'a str,
    },
    Triage {
        finding_id: &'a str,
        disposition: TriageDisposition,
        note: Option<&'a str>,
    },
    ProposeIssue {
        finding_id: &'a str,
        title: &'a str,
        summary: &'a str,
    },
}

/// Record one MetaTool invocation. This is best effort by design: a broken
/// local filesystem must not replace the original tool result, while the
/// structured log and coarse telemetry still make the failure visible.
pub fn record_invocation(
    input: &Value,
    context: InvocationContext<'_>,
    result: &anyhow::Result<crate::tool::ToolOutput>,
    duration_ms: u64,
) -> bool {
    let action = action_label(input);
    let input_bytes = serialized_len(input);
    let input_hash = digest_json(input);
    let input_shape = value_shape(input);
    let input_key_count = input.as_object().map_or(0, |object| object.len());

    let source = input.get("code").and_then(Value::as_str);
    let inputs = input.get("inputs");
    let source_bytes = source.map_or(0, str::len);
    let source_hash = source.map(digest_str);
    let inputs_bytes = inputs.map_or(0, serialized_len);
    let inputs_hash = inputs.map(digest_json);

    let (outcome, diagnostic_dump) = match result {
        Ok(_) => ("success", None),
        Err(error) => {
            let class = error_class(error.to_string().as_str());
            (
                class.outcome(),
                Some(DiagnosticDump {
                    error_class: class.as_str().to_string(),
                    message_bytes: error.to_string().len(),
                    message_hash: digest_str(&error.to_string()),
                }),
            )
        }
    };

    let (output_bytes, output_hash, metadata_bytes, metadata_hash, image_count) = match result {
        Ok(output) => {
            let metadata_bytes = output.metadata.as_ref().map_or(0, serialized_len);
            let metadata_hash = output.metadata.as_ref().map(digest_json);
            (
                output.output.len(),
                Some(digest_str(&output.output)),
                metadata_bytes,
                metadata_hash,
                output.images.len(),
            )
        }
        Err(_) => (0, None, 0, None, 0),
    };

    let trace = TraceRecord {
        schema_version: TRACE_SCHEMA_VERSION,
        trace_id: format!("trc_{}", Uuid::new_v4().simple()),
        recorded_at_ms: now_ms(),
        action: action.to_string(),
        outcome: outcome.to_string(),
        duration_ms,
        input_shape: input_shape.to_string(),
        input_key_count,
        input_bytes,
        input_hash,
        source_present: source.is_some(),
        source_bytes,
        source_hash,
        inputs_present: inputs.is_some(),
        inputs_bytes,
        inputs_hash,
        output_bytes,
        output_hash,
        metadata_bytes,
        metadata_hash,
        image_count,
        context_hash: digest_str(&format!(
            "{}\u{1f}{}\u{1f}{}",
            context.session_id, context.message_id, context.tool_call_id
        )),
        diagnostic_dump,
    };

    let persisted = persist_trace(context.working_dir, &trace).is_ok();
    let telemetry = jcode_base::telemetry::MetaToolTelemetry {
        action,
        outcome,
        duration_ms,
        source_bytes,
        input_bytes,
        output_bytes,
        input_shape,
        finding_action: is_finding_action(action),
    };
    jcode_base::telemetry::record_metatool_invocation(telemetry);

    let fields = [
        ("trace_id", trace.trace_id.clone()),
        ("action", action.to_string()),
        ("outcome", outcome.to_string()),
        (
            "duration_ms_bucket",
            bucket_duration_ms(duration_ms).to_string(),
        ),
        ("input_bytes_bucket", bucket_bytes(input_bytes).to_string()),
        (
            "source_bytes_bucket",
            bucket_bytes(source_bytes).to_string(),
        ),
        (
            "output_bytes_bucket",
            bucket_bytes(output_bytes).to_string(),
        ),
        ("trace_persisted", persisted.to_string()),
    ];
    if result.is_ok() {
        crate::logging::event_info("METATOOL_INVOCATION", fields);
    } else {
        crate::logging::event_warn("METATOOL_INVOCATION", fields);
    }

    persisted
}

/// Return a telemetry-safe shape for the raw input. This is intentionally
/// coarser than [`TraceRecord`] and contains no hashes or user-controlled text.
pub fn telemetry_input_shape(input: &Value) -> Value {
    json!({
        "shape": value_shape(input),
        "key_count": input.as_object().map_or(0, |object| object.len()),
        "source_present": input.get("code").is_some(),
        "inputs_present": input.get("inputs").is_some(),
        "finding_action": is_finding_action(action_label(input)),
    })
}

/// Execute a direct finding/reporting operation. All text is redacted before
/// it reaches the durable store. The proposal branch has no external adapter
/// by construction and returns an explicit local-only status.
pub fn execute_finding_action(
    action: FindingAction<'_>,
    context: FindingContext<'_>,
) -> Result<Value> {
    let _guard = OBSERVABILITY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = findings_path(context.working_dir)?;
    let mut findings = load_findings(&path)?;
    let now = now_ms();
    let initial_count = findings.len();
    prune_findings(&mut findings, now);
    if findings.len() != initial_count {
        write_findings(&path, &mut findings, now)?;
    }

    let result = match action {
        FindingAction::Report { title, summary } => {
            let finding = Finding {
                id: format!("fnd_{}", Uuid::new_v4().simple()),
                created_at_ms: now,
                updated_at_ms: now,
                context_hash: finding_context_hash(context),
                title: sanitize_finding_text(title, 160),
                summary: sanitize_finding_text(summary, MAX_FINDING_TEXT_CHARS),
                disposition: TriageDisposition::Unconfirmed,
                triage_note: None,
                triaged_at_ms: None,
                issue_proposals: Vec::new(),
            };
            if finding.title.is_empty() || finding.summary.is_empty() {
                bail!("finding title and summary are required")
            }
            findings.push(finding.clone());
            write_findings(&path, &mut findings, now)?;
            json!({
                "status": "recorded",
                "finding": finding,
            })
        }
        FindingAction::List { limit } => {
            findings.sort_by_key(|finding| Reverse(finding.updated_at_ms));
            let limit = limit.unwrap_or(50).clamp(1, 100);
            json!({
                "status": "ok",
                "findings": findings.into_iter().take(limit).collect::<Vec<_>>(),
                "limit": limit,
            })
        }
        FindingAction::Show { finding_id } => {
            let finding = find_finding(&findings, finding_id)?;
            json!({
                "status": "ok",
                "finding": finding,
            })
        }
        FindingAction::Triage {
            finding_id,
            disposition,
            note,
        } => {
            let finding = find_finding_mut(&mut findings, finding_id)?;
            finding.disposition = disposition;
            finding.triage_note = note.map(|value| sanitize_finding_text(value, 800));
            finding.triaged_at_ms = Some(now);
            finding.updated_at_ms = now;
            let updated = finding.clone();
            write_findings(&path, &mut findings, now)?;
            json!({
                "status": "triaged",
                "finding": updated,
            })
        }
        FindingAction::ProposeIssue {
            finding_id,
            title,
            summary,
        } => {
            let finding = find_finding_mut(&mut findings, finding_id)?;
            if !finding.disposition.allows_issue_proposal() {
                bail!("issue proposals require a suspected_bug or confirmed_bug triage disposition")
            }
            let proposal = IssueProposal {
                id: format!("ip_{}", Uuid::new_v4().simple()),
                finding_id: finding.id.clone(),
                created_at_ms: now,
                title: sanitize_finding_text(title, 160),
                summary: sanitize_finding_text(summary, MAX_FINDING_TEXT_CHARS),
                status: "local_only".to_string(),
                external_filing: false,
            };
            if proposal.title.is_empty() || proposal.summary.is_empty() {
                bail!("issue proposal title and summary are required")
            }
            finding.issue_proposals.push(proposal.clone());
            finding.updated_at_ms = now;
            write_findings(&path, &mut findings, now)?;
            json!({
                "status": "recorded_locally",
                "external_filing": false,
                "proposal": proposal,
            })
        }
    };

    Ok(result)
}

fn finding_context_hash(context: FindingContext<'_>) -> String {
    let workspace = context
        .working_dir
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "global".to_string());
    digest_str(&format!("{}\u{1f}{workspace}", context.session_id))
}

fn persist_trace(working_dir: Option<&Path>, trace: &TraceRecord) -> Result<()> {
    let _guard = OBSERVABILITY_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let path = traces_path(working_dir)?;
    let mut records = Vec::new();
    if let Ok(contents) = fs::read_to_string(&path) {
        for line in contents.lines() {
            if let Ok(record) = serde_json::from_str::<TraceRecord>(line) {
                records.push(record);
            }
        }
    }
    records.push(trace.clone());
    let now = trace.recorded_at_ms;
    records.retain(|record| now.saturating_sub(record.recorded_at_ms) <= RETENTION_MS);
    if records.len() > MAX_TRACE_RECORDS {
        let keep_from = records.len() - MAX_TRACE_RECORDS;
        records.drain(..keep_from);
    }

    let mut rendered = records
        .iter()
        .filter_map(|record| serde_json::to_string(record).ok())
        .collect::<Vec<_>>();
    while rendered.iter().map(String::len).sum::<usize>() + rendered.len() > MAX_TRACE_BYTES
        && rendered.len() > 1
    {
        rendered.remove(0);
    }
    write_atomic(&path, format!("{}\n", rendered.join("\n")).as_bytes())
}

fn findings_path(working_dir: Option<&Path>) -> Result<PathBuf> {
    Ok(observability_dir(working_dir)?.join(FINDINGS_FILE))
}

fn traces_path(working_dir: Option<&Path>) -> Result<PathBuf> {
    Ok(observability_dir(working_dir)?.join(TRACE_FILE))
}

#[cfg(test)]
pub(crate) fn trace_path_for_test(working_dir: Option<&Path>) -> Result<PathBuf> {
    traces_path(working_dir)
}

fn observability_dir(working_dir: Option<&Path>) -> Result<PathBuf> {
    let root = crate::storage::durable_state_dir().join("metatool-observability");
    let workspace = working_dir
        .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "global".to_string());
    let directory = root.join(digest_str(&workspace));
    crate::storage::ensure_dir(&directory)?;
    crate::platform::set_directory_permissions_owner_only(&directory)
        .context("harden local MetaTool observability directory")?;
    Ok(directory)
}

fn load_findings(path: &Path) -> Result<Vec<Finding>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| "parse local MetaTool findings".to_string()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error).context("read local MetaTool findings"),
    }
}

fn write_findings(path: &Path, findings: &mut Vec<Finding>, now: u64) -> Result<()> {
    prune_findings(findings, now);
    findings.sort_by_key(|finding| finding.updated_at_ms);
    write_atomic(path, serde_json::to_vec_pretty(findings)?.as_slice())
}

fn prune_findings(findings: &mut Vec<Finding>, now: u64) {
    findings.retain(|finding| now.saturating_sub(finding.updated_at_ms) <= RETENTION_MS);
    findings.sort_by_key(|finding| Reverse(finding.updated_at_ms));
    if findings.len() > MAX_FINDINGS {
        findings.truncate(MAX_FINDINGS);
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    fs::write(&temporary, bytes).context("write local MetaTool observability state")?;
    crate::platform::set_permissions_owner_only(&temporary)
        .context("harden temporary MetaTool observability state")?;
    fs::rename(&temporary, path).context("commit local MetaTool observability state")?;
    crate::platform::set_permissions_owner_only(path)
        .context("harden local MetaTool observability state")?;
    Ok(())
}

fn find_finding<'a>(findings: &'a [Finding], finding_id: &str) -> Result<&'a Finding> {
    findings
        .iter()
        .find(|finding| finding.id == finding_id)
        .ok_or_else(|| anyhow!("finding not found"))
}

fn find_finding_mut<'a>(findings: &'a mut [Finding], finding_id: &str) -> Result<&'a mut Finding> {
    findings
        .iter_mut()
        .find(|finding| finding.id == finding_id)
        .ok_or_else(|| anyhow!("finding not found"))
}

fn action_label(input: &Value) -> &'static str {
    match input.get("action").and_then(Value::as_str) {
        None => "evaluate",
        Some("status") => "status",
        Some("evaluate") => "evaluate",
        Some("guide") => "guide",
        Some("report_finding") => "report_finding",
        Some("list_findings") => "list_findings",
        Some("show_finding") => "show_finding",
        Some("triage_finding") => "triage_finding",
        Some("propose_issue") => "propose_issue",
        Some(_) => "unknown",
    }
}

fn is_finding_action(action: &str) -> bool {
    matches!(
        action,
        "report_finding" | "list_findings" | "show_finding" | "triage_finding" | "propose_issue"
    )
}

fn value_shape(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn serialized_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len())
}

fn digest_json(value: &Value) -> String {
    serde_json::to_vec(value)
        .map(|bytes| digest_bytes(&bytes))
        .unwrap_or_else(|_| digest_str("serialization_failed"))
}

fn digest_str(value: &str) -> String {
    digest_bytes(value.as_bytes())
}

fn digest_bytes(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    digest[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            duration.as_millis().min(u128::from(u64::MAX)) as u64
        })
}

fn bucket_duration_ms(value: u64) -> u64 {
    match value {
        0..=4 => 0,
        5..=24 => 5,
        25..=99 => 25,
        100..=499 => 100,
        500..=1_999 => 500,
        2_000..=9_999 => 2_000,
        _ => 10_000,
    }
}

fn bucket_bytes(value: usize) -> usize {
    match value {
        0 => 0,
        1..=64 => 64,
        65..=256 => 256,
        257..=1_024 => 1_024,
        1_025..=16_384 => 16_384,
        16_385..=65_536 => 65_536,
        _ => 65_537,
    }
}

#[derive(Debug, Clone, Copy)]
enum ErrorClass {
    InvalidInput,
    RuntimeFailure,
}

impl ErrorClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::RuntimeFailure => "runtime_failure",
        }
    }

    fn outcome(self) -> &'static str {
        self.as_str()
    }
}

fn error_class(message: &str) -> ErrorClass {
    let lower = message.to_ascii_lowercase();
    if [
        "invalid type",
        "missing field",
        "unknown variant",
        "code is required",
        "exceeds the",
        "must be",
        "requires a",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        ErrorClass::InvalidInput
    } else {
        ErrorClass::RuntimeFailure
    }
}

pub(crate) fn safe_error_class(error: &anyhow::Error) -> &'static str {
    error_class(&error.to_string()).as_str()
}

fn sanitizer_patterns() -> &'static Vec<(Regex, &'static str)> {
    REDACTION_PATTERNS.get_or_init(|| {
        vec![
            (
                Regex::new(r"(?is)-----BEGIN [^-]+-----.*?-----END [^-]+-----")
                    .expect("valid PEM redaction regex"),
                "<redacted>",
            ),
            (
                Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+")
                    .expect("valid bearer redaction regex"),
                "Bearer <redacted>",
            ),
            (
                Regex::new(r"(?i)\b(?:sk|pk|ghp|github_pat|xox[baprs]-)[A-Za-z0-9_-]{8,}\b")
                    .expect("valid token redaction regex"),
                "<redacted>",
            ),
            (
                Regex::new(r#"(?i)(api[_-]?key|access[_-]?token|refresh[_-]?token|token|secret|password|authorization|cookie|private[_-]?key)\s*[:=]\s*[\"']?[^,\s}\"']+"#)
                    .expect("valid key-value redaction regex"),
                "$1=<redacted>",
            ),
            (
                Regex::new(r"(?i)([?&](?:api[_-]?key|access[_-]?token|refresh[_-]?token|token|secret|password)=)[^&\s]+")
                    .expect("valid URL query redaction regex"),
                "$1<redacted>",
            ),
        ]
    })
}

/// Redact common secret forms, normalize controls, and cap local finding text.
/// The original length is not included in the returned text so truncation
/// cannot accidentally preserve a source snippet in diagnostic metadata.
pub fn sanitize_finding_text(value: &str, max_chars: usize) -> String {
    let mut redacted = value.to_string();
    for (pattern, replacement) in sanitizer_patterns() {
        redacted = pattern.replace_all(&redacted, *replacement).into_owned();
    }
    let normalized: String = redacted
        .chars()
        .map(|character| {
            if character.is_control() && character != '\n' && character != '\t' {
                ' '
            } else {
                character
            }
        })
        .collect();
    normalized
        .chars()
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context<'a>(root: &'a Path) -> FindingContext<'a> {
        FindingContext {
            session_id: "session-test",
            working_dir: Some(root),
        }
    }

    #[test]
    fn sanitizer_removes_secrets_and_truncates() {
        let text = "token=super-secret sk-test-secret-value \n".to_string() + &"x".repeat(100);
        let sanitized = sanitize_finding_text(&text, 40);
        assert!(!sanitized.contains("super-secret"));
        assert!(!sanitized.contains("sk-test-secret-value"));
        assert!(sanitized.chars().count() <= 40);
    }

    #[test]
    fn proposal_requires_bug_triage_and_is_local_only() {
        let _env = TestEnvironment::new();
        let workspace = tempfile::tempdir().expect("workspace");
        let context = context(workspace.path());
        let report = execute_finding_action(
            FindingAction::Report {
                title: "Observed maintenance drift token=super-secret-value",
                summary: "A bounded observation",
            },
            context,
        )
        .expect("report finding");
        let finding_id = report["finding"]["id"].as_str().expect("finding id");
        assert_eq!(report["finding"]["disposition"], json!("unconfirmed"));
        assert!(report["finding"]["contextHash"].as_str().is_some());
        assert!(!report.to_string().contains("super-secret-value"));
        assert!(!report.to_string().contains("session-test"));
        assert!(
            !report
                .to_string()
                .contains(workspace.path().to_string_lossy().as_ref())
        );

        let listed = execute_finding_action(FindingAction::List { limit: Some(10) }, context)
            .expect("list findings");
        assert_eq!(listed["findings"].as_array().expect("findings").len(), 1);
        let shown = execute_finding_action(FindingAction::Show { finding_id }, context)
            .expect("show finding");
        assert_eq!(shown["finding"]["id"], finding_id);

        let blocked = execute_finding_action(
            FindingAction::ProposeIssue {
                finding_id,
                title: "Should not file",
                summary: "Still unconfirmed",
            },
            context,
        )
        .expect_err("unconfirmed finding must not propose issue");
        assert!(blocked.to_string().contains("triage disposition"));

        execute_finding_action(
            FindingAction::Triage {
                finding_id,
                disposition: TriageDisposition::SuspectedBug,
                note: Some("Needs a reproduction"),
            },
            context,
        )
        .expect("triage finding");
        let proposal = execute_finding_action(
            FindingAction::ProposeIssue {
                finding_id,
                title: "Local proposal",
                summary: "Review this suspected bug",
            },
            context,
        )
        .expect("local proposal");
        assert_eq!(proposal["status"], "recorded_locally");
        assert_eq!(proposal["external_filing"], false);
        assert_eq!(proposal["proposal"]["status"], "local_only");
        assert_eq!(proposal["proposal"]["externalFiling"], false);

        let stored = fs::read_to_string(findings_path(Some(workspace.path())).expect("path"))
            .expect("stored findings");
        assert!(!stored.contains("super-secret-value"));
    }

    #[test]
    fn findings_are_pruned_to_bounded_count() {
        let _env = TestEnvironment::new();
        let workspace = tempfile::tempdir().expect("workspace");
        let context = context(workspace.path());
        for index in 0..(MAX_FINDINGS + 4) {
            execute_finding_action(
                FindingAction::Report {
                    title: "bounded",
                    summary: &format!("finding {index}"),
                },
                context,
            )
            .expect("report finding");
        }
        let listed = execute_finding_action(FindingAction::List { limit: None }, context)
            .expect("list findings");
        assert_eq!(listed["findings"].as_array().expect("findings").len(), 50);
        let stored = fs::read_to_string(findings_path(Some(workspace.path())).expect("path"))
            .expect("stored findings");
        let stored: Vec<Finding> = serde_json::from_str(&stored).expect("finding JSON");
        assert_eq!(stored.len(), MAX_FINDINGS);
    }

    struct TestEnvironment {
        _lock: std::sync::MutexGuard<'static, ()>,
        temp: tempfile::TempDir,
        previous_runtime_dir: Option<std::ffi::OsString>,
        previous_no_telemetry: Option<std::ffi::OsString>,
    }

    impl TestEnvironment {
        fn new() -> Self {
            let lock = crate::storage::lock_test_env();
            let temp = tempfile::tempdir().expect("test runtime directory");
            let previous_runtime_dir = std::env::var_os("JCODE_RUNTIME_DIR");
            let previous_no_telemetry = std::env::var_os("JCODE_NO_TELEMETRY");
            crate::env::set_var("JCODE_RUNTIME_DIR", temp.path());
            crate::env::set_var("JCODE_NO_TELEMETRY", "1");
            Self {
                _lock: lock,
                temp,
                previous_runtime_dir,
                previous_no_telemetry,
            }
        }
    }

    impl Drop for TestEnvironment {
        fn drop(&mut self) {
            match self.previous_runtime_dir.take() {
                Some(value) => crate::env::set_var("JCODE_RUNTIME_DIR", value),
                None => crate::env::remove_var("JCODE_RUNTIME_DIR"),
            }
            match self.previous_no_telemetry.take() {
                Some(value) => crate::env::set_var("JCODE_NO_TELEMETRY", value),
                None => crate::env::remove_var("JCODE_NO_TELEMETRY"),
            }
            let _ = self.temp.path();
        }
    }

    #[test]
    fn traces_are_pruned_to_bounded_count() {
        let _env = TestEnvironment::new();
        let workspace = tempfile::tempdir().expect("workspace");
        let output = Ok(crate::tool::ToolOutput::new("ok"));
        let context = InvocationContext {
            session_id: "session-test",
            message_id: "message-test",
            tool_call_id: "tool-test",
            working_dir: Some(workspace.path()),
        };
        for _ in 0..(MAX_TRACE_RECORDS + 4) {
            assert!(record_invocation(
                &json!({"action": "status"}),
                context,
                &output,
                0,
            ));
        }
        let trace_path = trace_path_for_test(Some(workspace.path())).expect("trace path");
        let lines = fs::read_to_string(trace_path)
            .expect("trace file")
            .lines()
            .count();
        assert_eq!(lines, MAX_TRACE_RECORDS);
    }
}
