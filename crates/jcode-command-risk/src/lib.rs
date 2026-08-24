//! Deterministic risk classification for shell commands.
//!
//! # Why this exists
//!
//! jcode executes `bash` tool calls with no gate of its own: the only check in
//! `ToolRegistry::execute` is an opt-in external `pre_tool` hook, which is off
//! by default. A model that decides to run `rm -rf ~` is obeyed immediately.
//! That is issue #604, where a user lost their home directory.
//!
//! # Design
//!
//! This crate is **stage 1** of a two-stage cascade: a cheap, deterministic,
//! high-recall filter. It never calls a model and never touches the network, so
//! it costs nothing on the overwhelmingly common safe path. Stage 2 (the
//! reflection gate) only runs when this returns something other than
//! [`RiskLevel::Safe`].
//!
//! Two deliberate choices:
//!
//! 1. **Classify by blast radius, not by command name.** A denylist of
//!    `rm -rf` misses `find -delete`, `shred`, `truncate`, `dd`, and `>file`.
//!    We ask "what would this destroy, and can it be undone" instead.
//! 2. **Bias hard toward recall.** A false positive costs one reflection turn.
//!    A false negative costs a home directory. When parsing is ambiguous we
//!    escalate rather than allow.
//!
//! # Honest limitations
//!
//! This is defense in depth, not a sandbox. A determined or unlucky
//! `sh -c "$(printf ...)"` can defeat any static parser, which is exactly why
//! [`RiskLevel::Confirm`] is a reflection prompt rather than a hard block, and
//! why the catastrophic tier is a small, absolute, path-based deny that does
//! not depend on parsing the command correctly.

mod gate;
mod paths;
mod tokenize;

pub use gate::{GateOutcome, Justification, gate};
pub use paths::{ProtectedPaths, is_catastrophic_target};
pub use tokenize::{Token, tokenize};

/// How dangerous a command looks, and therefore how much scrutiny it earns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// No destructive potential detected. Run immediately, no overhead.
    Safe,
    /// Destructive but bounded (inside the working directory, recoverable via
    /// git, or under a temp dir). Run, but record it.
    Low,
    /// Irreversible and reaches outside the working directory. Requires the
    /// model to re-justify against the user's actual request before running.
    Confirm,
    /// Would destroy the user's home, root, or credentials. Never runs, and no
    /// amount of model justification can unlock it.
    Catastrophic,
}

impl RiskLevel {
    /// Whether execution may proceed without a reflection turn.
    pub fn runs_immediately(self) -> bool {
        matches!(self, RiskLevel::Safe | RiskLevel::Low)
    }

    /// Whether any confirmation could ever unlock this.
    pub fn is_absolute_deny(self) -> bool {
        matches!(self, RiskLevel::Catastrophic)
    }
}

/// A specific reason a command was flagged, used to explain the refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskFinding {
    pub level: RiskLevel,
    /// Human-readable explanation, shown to the model verbatim.
    pub reason: String,
    /// The concrete path or argument that triggered this, when there is one.
    pub target: Option<String>,
}

/// The full verdict for one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub findings: Vec<RiskFinding>,
}

impl RiskAssessment {
    fn safe() -> Self {
        Self {
            level: RiskLevel::Safe,
            findings: Vec::new(),
        }
    }

    fn from_findings(findings: Vec<RiskFinding>) -> Self {
        let level = findings
            .iter()
            .map(|f| f.level)
            .max()
            .unwrap_or(RiskLevel::Safe);
        Self { level, findings }
    }

    /// The refusal text shown to the model, phrased to force a comparison
    /// against what the user actually asked for rather than a yes/no reflex.
    pub fn explanation(&self) -> String {
        let mut out = String::new();
        for finding in &self.findings {
            out.push_str("- ");
            out.push_str(&finding.reason);
            if let Some(target) = &finding.target {
                out.push_str(&format!(" (target: {target})"));
            }
            out.push('\n');
        }
        out
    }
}

/// Context needed to judge blast radius. Supplied by the caller because this
/// crate deliberately does no I/O of its own beyond path inspection.
#[derive(Debug, Clone, Default)]
pub struct RiskContext {
    /// The tool call's working directory, if any.
    pub working_dir: Option<std::path::PathBuf>,
    /// The user's home directory.
    pub home_dir: Option<std::path::PathBuf>,
}

impl RiskContext {
    pub fn from_env(working_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            working_dir,
            home_dir: dirs_home(),
        }
    }
}

fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from)
}

/// Commands that destroy data as their primary purpose.
///
/// Presence here does not by itself mean danger: `rm` inside the working
/// directory is routine. It means "inspect the targets".
const DESTRUCTIVE_COMMANDS: &[&str] = &[
    "rm", "rmdir", "shred", "unlink", "truncate", "dd", "mkfs", "fdisk", "parted", "wipefs", "srm",
    // Overwrite verbs. These destroy the *destination* just as surely as `rm`
    // does: `mv evil /etc/passwd` and `cp evil /etc/passwd` replace the file,
    // `tee` and `install` truncate it, and `ln -sf` replaces it with a symlink.
    // They were absent, so every one of them reported Safe against a protected
    // path.
    "mv", "cp", "tee", "install", "ln",
];

/// Commands that run another command. The real program is one of their
/// arguments, so `sudo rm -rf ~` must be unwrapped before classification or the
/// destructive verb is never seen at all.
const WRAPPER_COMMANDS: &[&str] = &[
    "sudo", "doas", "env", "nice", "ionice", "time", "timeout", "nohup", "xargs", "command",
    "builtin", "exec", "setsid", "stdbuf", "chroot", "su", "watch", "eval",
];

/// Wrapper options that consume the following word as their value.
///
/// This is deliberately **per wrapper**. A shared list is unsound: `sudo -n`
/// means "non-interactive" and takes no value, while `nice -n` takes a
/// priority. Treating `sudo -n rm -rf ~` as "`-n` eats `rm`" made the
/// destructive verb invisible and the whole command Safe.
const WRAPPER_FLAGS_WITH_VALUES: &[(&str, &[&str])] = &[
    ("nice", &["-n", "--adjustment"]),
    ("ionice", &["-n", "-c", "-p"]),
    ("timeout", &["-k", "-s", "--signal"]),
    ("env", &["-u", "--unset"]),
    ("xargs", &["-n", "-P", "-I", "-d", "-s", "-L", "-a", "-E"]),
    ("stdbuf", &["-i", "-o", "-e"]),
    ("setsid", &[]),
    ("nohup", &[]),
    ("time", &["-f", "-o"]),
    ("watch", &["-n", "--interval"]),
    // `sudo`/`doas` short flags here take a value; `-n`, `-s`, `-k`, `-i` do not.
    (
        "sudo",
        &[
            "-u", "-g", "-p", "-C", "-h", "-r", "-t", "-U", "--user", "--group",
        ],
    ),
    ("doas", &["-u", "-C"]),
    ("command", &[]),
    ("builtin", &[]),
    ("exec", &["-a"]),
];

/// Whether `flag` consumes the next word when it appears after `wrapper`.
fn flag_takes_value(wrapper: &str, flag: &str) -> bool {
    WRAPPER_FLAGS_WITH_VALUES
        .iter()
        .find(|(name, _)| *name == wrapper)
        .is_some_and(|(_, flags)| flags.contains(&flag))
}

/// Shell grouping punctuation that can precede a program name.
///
/// `(rm -rf ~)` and `{ rm -rf ~; }` are ordinary shell, and the tokenizer
/// leaves the bracket glued to the program (`(rm`) or standing alone (`{`).
/// Either way the program lookup misses and the command reads as Safe.
fn strip_command_prefix(mut tokens: &[Token]) -> &[Token] {
    loop {
        let Some(first) = tokens.first() else {
            return tokens;
        };
        // A leading `VAR=value` assignment is environment, not the program.
        // Guard against eating a real operand: only a bare `name=value` with
        // no slash qualifies (`of=/dev/sda` is a `dd` operand, not a prefix,
        // but it never appears in leading position).
        let is_assignment = !first.is_operator
            && first.text.split_once('=').is_some_and(|(key, _)| {
                !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
        if is_assignment {
            tokens = &tokens[1..];
            continue;
        }
        // A standalone grouping token contributes no program.
        if matches!(first.text.as_str(), "(" | "{" | ")" | "}" | "!") {
            tokens = &tokens[1..];
            continue;
        }
        return tokens;
    }
}

/// Strip grouping punctuation glued to a program name, so `(rm` reads as `rm`.
///
/// `${HOME}` must survive intact: trimming its trailing `}` would leave
/// `${HOME` unexpandable and downgrade a home-directory delete from
/// Catastrophic to Confirm, so a token containing `${` is left alone.
fn unwrap_group_punctuation(name: &str) -> &str {
    if name.contains("${") {
        return name;
    }
    name.trim_start_matches(['(', '{', '!'])
        .trim_end_matches([')', '}', ';'])
}

/// Pull the inner command text out of `$(...)` and backtick substitutions.
///
/// Nesting is tracked for `$(`, so `$(echo $(rm -rf ~))` yields the whole
/// inner text and the recursive assessment finds the `rm`. An unterminated
/// substitution yields the remainder, which is the conservative choice: it is
/// better to assess too much text than to silently skip a command.
fn extract_substitutions(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '$' && i + 1 < bytes.len() && bytes[i + 1] == '(' {
            let mut depth = 1;
            let mut j = i + 2;
            let start = j;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    j += 1;
                }
            }
            found.push(bytes[start..j.min(bytes.len())].iter().collect());
            i = j + 1;
            continue;
        }
        if bytes[i] == '`' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j] != '`' {
                j += 1;
            }
            found.push(bytes[start..j.min(bytes.len())].iter().collect());
            i = j + 1;
            continue;
        }
        i += 1;
    }
    found
}

/// Shells, which take their program from a string argument we cannot parse
/// reliably. Treated as opaque rather than assumed safe.
const SHELL_COMMANDS: &[&str] = &["sh", "bash", "zsh", "dash", "ksh", "fish"];

/// Commands that are destructive only with specific flags.
const CONDITIONALLY_DESTRUCTIVE: &[(&str, &[&str])] = &[
    ("find", &["-delete"]),
    // `git clean` deletes untracked files; `reset --hard` and `checkout -- .`
    // discard uncommitted work, which is unrecoverable in exactly the way this
    // gate exists to catch.
    ("git", &["clean", "--hard"]),
    ("chmod", &["-R"]),
    ("chown", &["-R"]),
    // `rsync --delete` removes files from the destination that are absent at
    // the source, which can empty a whole tree.
    ("rsync", &["--delete", "--delete-after", "--delete-before"]),
];

/// Assess a single shell command string.
///
/// This is the crate's entry point and is intentionally total: any input,
/// including garbage, produces an assessment rather than an error.
pub fn assess(command: &str, ctx: &RiskContext) -> RiskAssessment {
    let mut findings = Vec::new();

    for segment in tokenize::split_segments(command) {
        assess_segment(&segment, ctx, &mut findings);
    }

    // Command substitution runs a full command regardless of how harmless the
    // surrounding one is: `echo $(rm -rf ~)` deletes the home directory and
    // then echoes nothing. This runs on the raw string rather than per token
    // because the tokenizer splits `$(rm -rf ~)` on whitespace into `$(rm`,
    // `-rf`, `~)`, so no single token holds the substitution.
    //
    // Nested substitutions are followed to a bounded depth: the extractor
    // returns the outer body, whose own `$(...)` must then be unwrapped again
    // or `echo $(echo $(rm -rf ~))` slips through. The bound keeps a
    // pathological input from looping.
    let mut pending: Vec<String> = extract_substitutions(command);
    let mut depth = 0;
    while let Some(inner) = pending.pop() {
        if inner.trim().is_empty() {
            continue;
        }
        for segment in tokenize::split_segments(&inner) {
            assess_segment(&segment, ctx, &mut findings);
        }
        if depth < 8 {
            depth += 1;
            pending.extend(extract_substitutions(&inner));
        }
    }

    if findings.is_empty() {
        return RiskAssessment::safe();
    }
    RiskAssessment::from_findings(findings)
}

fn assess_segment(tokens: &[Token], ctx: &RiskContext, findings: &mut Vec<RiskFinding>) {
    // Strip wrapper programs (`sudo`, `env`, `xargs`, ...) so the destructive
    // verb underneath is the one we classify. Without this, any common prefix
    // is a complete bypass.
    let mut tokens = tokens;
    let mut wrapped_by: Option<String> = None;

    // Leading `VAR=value` assignments and shell grouping punctuation come
    // before the program name. Both were previously only stripped *inside*
    // wrapper unwrapping, so `FOO=1 rm -rf ~` and `(rm -rf ~)` presented a
    // program name of `FOO=1` / `(rm`, matched nothing in the destructive
    // list, and were reported Safe. That is a one-token bypass of the entire
    // gate, so the stripping has to happen before the first program lookup.
    let stripped = strip_command_prefix(tokens);
    tokens = stripped;

    loop {
        let Some(first) = tokens.first() else {
            // Ran off the end while unwrapping: the payload is invisible.
            if let Some(wrapper) = wrapped_by {
                findings.push(RiskFinding {
                    level: RiskLevel::Confirm,
                    reason: format!(
                        "`{wrapper}` runs another command that could not be \
                         identified statically"
                    ),
                    target: None,
                });
            }
            return;
        };
        let name = unwrap_group_punctuation(&first.basename()).to_string();
        if !WRAPPER_COMMANDS.contains(&name.as_str()) {
            break;
        }
        wrapped_by = Some(name.clone());
        // `su` and `eval` carry their payload as a quoted string argument, so
        // assess it as script text the way a shell's `-c` string is assessed.
        // Landing on a program token instead walked off the end and returned
        // Safe.
        if matches!(name.as_str(), "su" | "eval") {
            for token in tokens.iter().skip(1).filter(|t| !t.is_flag()) {
                for segment in tokenize::split_segments(&token.text) {
                    assess_segment(&segment, ctx, findings);
                }
            }
            return;
        }
        // `chroot NEWROOT CMD ...` takes a directory operand *before* the
        // command it runs. Skipping only flags left `chroot` pointing at the
        // directory instead of the command, so `chroot /mnt rm -rf ~` read as
        // a chroot of a path and never saw the `rm`.
        if name == "chroot" {
            let rest = &tokens[1..];
            let mut idx = 0;
            while idx < rest.len() && (rest[idx].is_flag() || rest[idx].is_operator) {
                idx += 1;
            }
            // Consume the NEWROOT operand itself.
            if idx < rest.len() {
                idx += 1;
            }
            tokens = &rest[idx..];
            continue;
        }
        // Skip the wrapper plus its own options and `VAR=value` assignments,
        // landing on the wrapped program. Options that take a separate value
        // (`nice -n 10`, `timeout 5`) must consume that value too.
        let rest = &tokens[1..];
        let mut idx = 0;
        while idx < rest.len() {
            let token = &rest[idx];
            if token.is_operator || token.text.contains('=') {
                idx += 1;
                continue;
            }
            if token.is_flag() {
                idx += 1;
                // A short flag known to take an argument consumes the next word.
                if flag_takes_value(&name, &token.text) && idx < rest.len() {
                    idx += 1;
                }
                continue;
            }
            // A bare number is an operand of the wrapper itself (`timeout 5`),
            // not the program to run.
            if token.text.chars().all(|c| c.is_ascii_digit() || c == '.') {
                idx += 1;
                continue;
            }
            break;
        }
        tokens = &rest[idx..];
    }

    let Some(program) = tokens.first() else {
        // A wrapper with nothing recognizable after it hides its payload.
        if let Some(wrapper) = wrapped_by {
            findings.push(RiskFinding {
                level: RiskLevel::Confirm,
                reason: format!(
                    "`{wrapper}` runs another command that could not be \
                     identified statically"
                ),
                target: None,
            });
        }
        return;
    };
    let program_name = unwrap_group_punctuation(&program.basename()).to_string();

    // A shell invoked with an inline script is opaque to this parser. Assess
    // the script text too, so `sh -c "rm -rf ~"` is not a free pass.
    if SHELL_COMMANDS.contains(&program_name.as_str()) {
        for token in tokens.iter().skip(1).filter(|t| !t.is_flag()) {
            for segment in tokenize::split_segments(&token.text) {
                assess_segment(&segment, ctx, findings);
            }
        }
        return;
    }

    let is_destructive = DESTRUCTIVE_COMMANDS.contains(&program_name.as_str());
    let conditional_flags = CONDITIONALLY_DESTRUCTIVE
        .iter()
        .find(|(name, _)| *name == program_name)
        .map(|(_, flags)| *flags);

    let triggered = if is_destructive {
        true
    } else if let Some(flags) = conditional_flags {
        tokens.iter().any(|t| flags.contains(&t.text.as_str()))
    } else {
        false
    };

    // Output redirection truncates a file even with a harmless program.
    let redirect_targets: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.is_truncating_redirect_target)
        .collect();

    if !triggered && redirect_targets.is_empty() {
        return;
    }

    // Only a destructive program's own operands are things it would delete. A
    // harmless program that merely redirects (`ls dir 2>/dev/null`) clobbers
    // exactly the redirect destination and nothing else, so its operands must
    // not be classified as deletion targets. Conflating the two reported every
    // ordinary read-only command that redirected stderr as destroying its
    // arguments.
    let mut targets: Vec<&Token> = if triggered {
        tokens
            .iter()
            .skip(1)
            .filter(|t| !t.is_flag() && !t.is_operator)
            .collect()
    } else {
        Vec::new()
    };
    // `find -name '*.rs'` / `-path '*/x/*'` take a *pattern*, not a path. The
    // pattern was being read as a glob target, so an ordinary
    // `find . -name '*.rs' -exec grep ...` asked for confirmation.
    if program_name == "find" {
        let pattern_values: Vec<&str> = tokens
            .windows(2)
            .filter(|w| {
                matches!(
                    w[0].text.as_str(),
                    "-name" | "-path" | "-iname" | "-ipath" | "-regex"
                )
            })
            .map(|w| w[1].text.as_str())
            .collect();
        targets.retain(|t| !pattern_values.contains(&t.text.as_str()));
    }
    targets.extend(redirect_targets.iter().copied());

    // A destructive command fed by a pipe takes its operands from the previous
    // command's output, which we cannot enumerate. `find ~ -type f | xargs rm`
    // is a real deletion of home contents that neither segment reveals on its
    // own, so escalate rather than trust the visible arguments.
    if triggered && tokens.first().is_some_and(|t| t.receives_pipe) {
        findings.push(RiskFinding {
            level: RiskLevel::Confirm,
            reason: format!(
                "`{program_name}` deletes paths supplied by a pipe, so the set \
                 of affected files cannot be checked before it runs"
            ),
            target: None,
        });
    }

    // A destructive program with no parsable target is more suspicious, not
    // less: we could not see what it would touch.
    if triggered && targets.is_empty() {
        findings.push(RiskFinding {
            level: RiskLevel::Confirm,
            reason: format!(
                "`{program_name}` is destructive but its target could not be \
                 determined statically, so its blast radius is unknown"
            ),
            target: None,
        });
        return;
    }

    // Recursion only widens blast radius for a program that is itself
    // destructive. `grep -rn`, `ls -R`, and `cp -r` carry an `-r` that says
    // nothing about deletion, and reading it as one reported plain searches as
    // "recursive delete inside the working directory".
    let recursive = triggered && tokens.iter().any(|t| t.is_recursive_flag());

    for target in targets {
        // Grouping punctuation glues onto the last operand of a subshell, so
        // `(rm -rf ~)` yields the token `~)`. Left attached, `~)` never equals
        // `~` and the home directory check misses.
        let text = unwrap_group_punctuation(&target.text).to_string();
        // `dd`-style `key=value` operands hide the path from a naive scan.
        let raw = text
            .split_once('=')
            .filter(|(key, _)| matches!(*key, "of" | "if" | "seek" | "conv"))
            .map(|(_, value)| value)
            .unwrap_or(&text);
        let expanded = paths::expand(raw, ctx);
        if let Some(finding) = paths::classify_target(&expanded, raw, recursive, ctx) {
            findings.push(finding);
        }
    }
}

#[cfg(test)]
#[path = "assess_tests.rs"]
mod assess_tests;
