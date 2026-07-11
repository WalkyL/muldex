use clap::Parser;
use clap::Subcommand;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use muldex_core::protocol::CapabilityRegistrySnapshot;
use muldex_core::protocol::ContinueDecision;
use muldex_core::protocol::ContinueReason;
use muldex_core::protocol::ContinueRequest;
use muldex_core::protocol::CycleSummary;
use muldex_core::protocol::CheckpointRef;
use muldex_core::protocol::ContextPressure;
use muldex_core::protocol::ContinueMode;
use muldex_core::protocol::ExecutionMode;
use muldex_core::protocol::InterruptInjectionMode;
use muldex_core::protocol::InterruptKind;
use muldex_core::protocol::InterruptQueueState;
use muldex_core::protocol::MediaAssetRef;
use muldex_core::protocol::MediaContextEnvelope;
use muldex_core::protocol::MediaKind;
use muldex_core::protocol::MediaSource;
use muldex_core::protocol::PendingApprovalState;
use muldex_core::protocol::PendingInterrupt;
use muldex_core::protocol::PermissionActionKind;
use muldex_core::protocol::PermissionDecision;
use muldex_core::protocol::PermissionDecisionStatus;
use muldex_core::protocol::PermissionContextSnapshot;
use muldex_core::protocol::PermissionRequest;
use muldex_core::protocol::PermissionUrgency;
use muldex_core::protocol::PostCompactionState;
use muldex_core::protocol::ProgressSnapshot;
use muldex_core::protocol::RecoveryReason;
use muldex_core::protocol::RecoverySnapshot;
use muldex_core::protocol::RunOutcome;
use muldex_core::protocol::RunReport;
use muldex_core::protocol::RuntimeModeState;
use muldex_core::protocol::SandboxModeDescriptor;
use muldex_core::protocol::SelfCorrectionState;
use muldex_core::protocol::SkillInvocationState;
use muldex_core::protocol::ApprovalPolicyDescriptor;
use muldex_core::protocol::CodexSessionContinuationSnapshot;
use muldex_core::reasoning_harness::EscalationPolicy;
use muldex_core::reasoning_harness::ProhibitionRule;
use muldex_core::reasoning_harness::ReasoningHarnessRequest;
use muldex_core::reasoning_harness::decide_reasoning_harness;
use muldex_core::upstream_adapter::CodexBootstrapSnapshot;
use muldex_core::upstream_adapter::CodexLiveContinuationSnapshot;
use muldex_core::upstream_adapter::CodexSignalSnapshot;
use muldex_core::upstream_adapter::codex_bootstrap_snapshot_to_harness_request;
use muldex_core::upstream_adapter::codex_live_snapshot_to_harness_request;
use muldex_core::upstream_adapter::codex_snapshot_to_harness_request;
use muldex_runtime::continuity::ExternalRuntimeSnapshot;
use muldex_runtime::continuity::ReportExportMode;
use muldex_runtime::continuity::export_session;
use muldex_runtime::continuity::export_session_view;
use muldex_runtime::continuity::export_host;
use muldex_runtime::continuity::import_external_snapshot_as_runtime_state;
use muldex_runtime::client_views::command_receipt_view;
use muldex_runtime::client_views::command_envelope_view;
use muldex_runtime::client_views::daemon_status_view;
use muldex_runtime::client_views::ClientCommandView;
use muldex_runtime::client_views::inspect_session_view;
use muldex_runtime::client_views::project_client_command;
use muldex_runtime::client_views::response_view;
use muldex_runtime::client_views::session_list_view;
use muldex_runtime::client_policy::ClientAccessMode;
use muldex_runtime::client_policy::client_command_allowed;
use muldex_runtime::daemon::RuntimeDaemon;
use muldex_runtime::daemon_transport::DaemonCommandEnvelope;
use muldex_runtime::daemon_transport::FileCommandTransport;
use muldex_runtime::host::RuntimeHost;
use muldex_runtime::daemon_local::StaleOwnershipStatus;
use muldex_runtime::runtime::RuntimeState;
use muldex_runtime::runtime::RuntimeStepResult;
use muldex_runtime::runtime::RuntimeCommand;
use muldex_runtime::runtime::RuntimeCommandResult;
use muldex_runtime::runtime::RuntimeDriveResult;
use muldex_runtime::runtime::RuntimeEvent;
use muldex_runtime::runtime::RuntimeDriver;
use muldex_runtime::runtime::RuntimePhase;
use serde::Deserialize;
use serde::Serialize;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::fs;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(
    name = "muldex",
    subcommand_negates_reqs = true,
    override_usage = "muldex [PROMPT]\n       muldex <COMMAND> [ARGS]"
)]
struct Cli {
    #[arg(value_name = "PROMPT")]
    prompt: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum Scenario {
    Healthy,
    NoProgress,
    RecoverableFailure,
    PostCompactionStall,
    MediaHeavy,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum SessionExportModeArg {
    Raw,
    Compressed,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum DaemonCommandKindArg {
    Status,
    AdvanceSample,
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum ClientAccessModeArg {
    ReadOnly,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveSlashCommand {
    Model(Option<String>),
    Approval(Option<String>),
    Compact,
    Sessions,
    Resume(Option<String>),
    New,
    ConfigLlm(InteractiveLlmConfigCommand),
    Provider(InteractiveProviderCommand),
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveLlmConfigCommand {
    Show,
    Test,
    SetHost(String),
    SetPort(u16),
    SetApiKey(String),
    SetDefaultModel(String),
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveProviderCommand {
    Show,
    List,
    Use(String),
    Test(Option<String>),
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveShellInput {
    Empty,
    Exit,
    Help,
    Status,
    SlashCommand(InteractiveSlashCommand),
    Prompt(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct InteractiveShellState {
    model: String,
    approval_mode: String,
    compact_count: u32,
    resume_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum InteractiveMessageRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct InteractiveMessage {
    role: InteractiveMessageRole,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct InteractiveShellSnapshot {
    schema_version: String,
    session_id: String,
    shell: InteractiveShellState,
    runtime: RuntimeState,
    messages: Vec<InteractiveMessage>,
    #[serde(default)]
    prompt_history: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct InteractiveShellStore {
    schema_version: String,
    active_session_id: String,
    sessions: Vec<InteractiveShellSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct ProviderConfig {
    kind: String,
    host: Option<String>,
    port: Option<u16>,
    base_url: Option<String>,
    api_key: Option<String>,
    api_key_env: Option<String>,
    default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct LegacyLlmRouterConfig {
    host: String,
    port: u16,
    api_key: String,
    default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct MuldexConfig {
    schema_version: String,
    #[serde(default)]
    default_provider: Option<String>,
    #[serde(default)]
    providers: std::collections::BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    llm_router: Option<LegacyLlmRouterConfig>,
}

struct RawModeGuard {
    active: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InteractivePromptBuffer {
    text: String,
    cursor: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InteractiveSlashCompletionState {
    seed: String,
    matches: Vec<&'static str>,
    index: usize,
    visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InteractiveSlashHint {
    command: &'static str,
    summary: &'static str,
}

#[derive(Debug, Clone, Default)]
struct InteractiveScriptedKeysState {
    events: std::collections::VecDeque<KeyEvent>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InteractiveHistoryState {
    entries: Vec<String>,
    index_from_end: Option<usize>,
    draft: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct InteractiveHistorySearchState {
    active: bool,
    draft: Option<String>,
    query: String,
    matches: Vec<usize>,
    match_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InteractiveKeyAction {
    Noop,
    RedrawPrompt,
    RedrawFrame,
    Exit,
    Status,
    Submit(InteractiveShellInput),
}

impl RawModeGuard {
    fn activate() -> Result<Self, Box<dyn std::error::Error>> {
        enable_raw_mode()?;
        Ok(Self { active: true })
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = disable_raw_mode();
        }
    }
}

impl InteractivePromptBuffer {
    fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    fn move_home(&mut self) {
        self.cursor = 0;
    }

    fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    fn first_line(&self) -> &str {
        self.text.lines().next().unwrap_or("")
    }

    fn replace_first_line(&mut self, replacement: &str) {
        let first_line_len = self.first_line().len();
        let tail = self.text[first_line_len..].to_string();
        let cursor = if self.cursor <= first_line_len {
            replacement.len()
        } else {
            self.cursor
                .saturating_add_signed(replacement.len() as isize - first_line_len as isize)
        };

        self.text = format!("{replacement}{tail}");
        self.cursor = cursor.min(self.text.len());
    }

    fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor = self.cursor.saturating_add(ch.len_utf8());
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let previous = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.text.drain(previous..self.cursor);
        self.cursor = previous;
    }

    fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        self.cursor = self.text[..self.cursor]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0);
    }

    fn move_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(index, _)| self.cursor + index)
            .unwrap_or(self.text.len());
        self.cursor = next;
    }

    fn move_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let prefix = &self.text[..self.cursor];
        let mut chars = prefix.char_indices().collect::<Vec<_>>();
        while let Some((index, ch)) = chars.pop() {
            if !ch.is_whitespace() {
                self.cursor = index;
                break;
            }
        }
        while self.cursor > 0 {
            let prefix = &self.text[..self.cursor];
            let Some((index, ch)) = prefix.char_indices().last() else {
                break;
            };
            if ch.is_whitespace() {
                break;
            }
            self.cursor = index;
        }
    }

    fn move_word_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }

        let mut seen_non_whitespace = false;
        for (offset, ch) in self.text[self.cursor..].char_indices() {
            if ch.is_whitespace() {
                if seen_non_whitespace {
                    self.cursor += offset;
                    while self.cursor < self.text.len() {
                        let Some(next_char) = self.text[self.cursor..].chars().next() else {
                            break;
                        };
                        if !next_char.is_whitespace() {
                            break;
                        }
                        self.cursor += next_char.len_utf8();
                    }
                    return;
                }
            } else {
                seen_non_whitespace = true;
            }
        }
        self.cursor = self.text.len();
    }

    fn delete_word_left(&mut self) {
        let original_cursor = self.cursor;
        self.move_word_left();
        if self.cursor < original_cursor {
            self.text.drain(self.cursor..original_cursor);
        }
    }
}

impl InteractiveSlashCompletionState {
    fn reset(&mut self) {
        self.seed.clear();
        self.matches.clear();
        self.index = 0;
        self.visible = false;
    }

    fn update_from_buffer(&mut self, buffer: &InteractivePromptBuffer) {
        self.seed = buffer.first_line().to_string();
        self.matches = interactive_slash_catalog()
            .iter()
            .filter(|hint| hint.command.starts_with(buffer.first_line()))
            .map(|hint| hint.command)
            .collect();
        self.index = 0;
        self.visible = !self.matches.is_empty();
    }

    fn current_command(&self) -> Option<&'static str> {
        self.matches.get(self.index).copied()
    }

    fn select_next(&mut self) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        self.index = (self.index + 1) % self.matches.len();
        self.visible = true;
        true
    }

    fn select_previous(&mut self) -> bool {
        if self.matches.is_empty() {
            return false;
        }
        self.index = if self.index == 0 {
            self.matches.len().saturating_sub(1)
        } else {
            self.index.saturating_sub(1)
        };
        self.visible = true;
        true
    }
}

impl InteractiveHistoryState {
    fn from_entries(entries: Vec<String>) -> Self {
        Self {
            entries,
            index_from_end: None,
            draft: None,
        }
    }

    fn record_submission(&mut self, entry: &str) {
        let normalized = entry.trim();
        if normalized.is_empty() {
            self.index_from_end = None;
            self.draft = None;
            return;
        }
        if self.entries.last().is_none_or(|last| last != normalized) {
            self.entries.push(normalized.to_string());
        }
        self.index_from_end = None;
        self.draft = None;
    }

    fn previous(&mut self, buffer: &mut InteractivePromptBuffer) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        if self.index_from_end.is_none() {
            self.draft = Some(buffer.text.clone());
        }
        let next_index = self.index_from_end.unwrap_or(0).saturating_add(1).min(self.entries.len());
        self.index_from_end = Some(next_index);
        let entry = self.entries[self.entries.len() - next_index].clone();
        buffer.text = entry;
        buffer.cursor = buffer.text.len();
        true
    }

    fn next(&mut self, buffer: &mut InteractivePromptBuffer) -> bool {
        let Some(current_index) = self.index_from_end else {
            return false;
        };

        if current_index <= 1 {
            self.index_from_end = None;
            buffer.text = self.draft.clone().unwrap_or_default();
            buffer.cursor = buffer.text.len();
            self.draft = None;
            return true;
        }

        let next_index = current_index - 1;
        self.index_from_end = Some(next_index);
        let entry = self.entries[self.entries.len() - next_index].clone();
        buffer.text = entry;
        buffer.cursor = buffer.text.len();
        true
    }
}

impl InteractiveHistorySearchState {
    fn reset(&mut self) {
        self.active = false;
        self.draft = None;
        self.query.clear();
        self.matches.clear();
        self.match_index = 0;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn set_query(
        &mut self,
        history: &InteractiveHistoryState,
        buffer: &mut InteractivePromptBuffer,
        query: String,
    ) -> bool {
        let normalized = query.trim().to_string();
        self.query = normalized.clone();
        self.matches = history
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.contains(&normalized))
            .map(|(index, _)| index)
            .collect();
        self.match_index = self.matches.len().saturating_sub(1);
        self.active = true;

        let Some(entry_index) = self.matches.get(self.match_index).copied() else {
            return false;
        };
        buffer.text = history.entries[entry_index].clone();
        buffer.cursor = buffer.text.len();
        true
    }

    fn reverse_search(
        &mut self,
        history: &InteractiveHistoryState,
        buffer: &mut InteractivePromptBuffer,
    ) -> bool {
        if !self.active {
            self.active = true;
            self.draft = Some(buffer.text.clone());
        }

        let query = if self.matches.is_empty() {
            buffer.text.clone()
        } else {
            self.query.clone()
        };
        let normalized = query.trim().to_string();
        if normalized.is_empty() {
            return false;
        }

        if self.matches.is_empty() || self.query != normalized {
            self.query = normalized.clone();
            self.matches = history
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| entry.contains(&normalized))
                .map(|(index, _)| index)
                .collect();
            self.match_index = self.matches.len().saturating_sub(1);
        } else if !self.matches.is_empty() {
            if self.match_index == 0 && self.matches.len() > 1 {
                self.match_index = self.matches.len() - 1;
            } else {
                self.match_index = self.match_index.saturating_sub(1);
            }
        }

        let Some(entry_index) = self.matches.get(self.match_index).copied() else {
            self.active = true;
            return false;
        };

        buffer.text = history.entries[entry_index].clone();
        buffer.cursor = buffer.text.len();
        true
    }

    fn backspace_query(
        &mut self,
        history: &InteractiveHistoryState,
        buffer: &mut InteractivePromptBuffer,
    ) -> bool {
        if !self.active {
            return false;
        }
        let mut query = self.query.clone();
        query.pop();
        if query.is_empty() {
            buffer.text = self.draft.clone().unwrap_or_default();
            buffer.cursor = buffer.text.len();
            self.reset();
            return true;
        }
        self.set_query(history, buffer, query)
    }

    fn extend_query(
        &mut self,
        history: &InteractiveHistoryState,
        buffer: &mut InteractivePromptBuffer,
        ch: char,
    ) -> bool {
        if !self.active {
            return false;
        }
        let mut query = self.query.clone();
        query.push(ch);
        self.set_query(history, buffer, query)
    }

    fn restore_draft(&mut self, buffer: &mut InteractivePromptBuffer) -> bool {
        if !self.active {
            return false;
        }
        buffer.text = self.draft.clone().unwrap_or_default();
        buffer.cursor = buffer.text.len();
        self.reset();
        true
    }

    fn current_match<'a>(&self, history: &'a InteractiveHistoryState) -> Option<&'a str> {
        let entry_index = *self.matches.get(self.match_index)?;
        history.entries.get(entry_index).map(String::as_str)
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    DecideSample {
        #[arg(long, value_enum, default_value = "healthy")]
        scenario: Scenario,
    },
    DecideFile { path: PathBuf },
    DecideCodexSnapshot { path: PathBuf },
    DecideWorkspace {
        #[arg(long)]
        workspace: PathBuf,
        #[arg(long)]
        objective: Option<String>,
        #[arg(long = "objective-file")]
        objective_file: Option<PathBuf>,
        #[arg(long, default_value = "build")]
        mode: String,
        #[arg(long, default_value_t = 0)]
        no_progress_iterations: u32,
        #[arg(long, default_value_t = false)]
        post_compaction: bool,
        #[arg(long, default_value_t = false)]
        recoverable_failure: bool,
        #[arg(long, default_value_t = false)]
        print_request: bool,
    },
    DemoApprovalResume,
    DemoHostPersistence,
    SaveHostSnapshot {
        #[arg(long)]
        path: PathBuf,
    },
    LoadHostSnapshot {
        #[arg(long)]
        path: PathBuf,
    },
    ImportCodexSnapshot {
        #[arg(long)]
        path: PathBuf,
    },
    ExportSessionView {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "session-id")]
        session_id: String,
        #[arg(long, value_enum, default_value = "raw")]
        mode: SessionExportModeArg,
    },
    DaemonBootEmpty {
        #[arg(long)]
        path: PathBuf,
    },
    DaemonBootLoad {
        #[arg(long)]
        path: PathBuf,
    },
    DaemonSave {
        #[arg(long)]
        path: PathBuf,
    },
    DaemonStatus {
        #[arg(long)]
        path: PathBuf,
    },
    DaemonServeOnce {
        #[arg(long)]
        path: PathBuf,
    },
    DaemonServeLoop {
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 1)]
        iterations: usize,
    },
    DaemonSendCommand {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "command-id")]
        command_id: String,
        #[arg(long = "session-id")]
        session_id: String,
        #[arg(long, value_enum)]
        kind: DaemonCommandKindArg,
    },
    DaemonReadResponse {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "command-id")]
        command_id: String,
    },
    DaemonStaleStatus {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "threshold-ms", default_value_t = 60_000)]
        threshold_ms: u64,
    },
    DaemonForceTakeover {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "threshold-ms", default_value_t = 60_000)]
        threshold_ms: u64,
    },
    ServerForeground {
        #[arg(long)]
        path: PathBuf,
        #[arg(long, default_value_t = 1)]
        iterations: usize,
    },
    ClientStatus {
        #[arg(long)]
        path: PathBuf,
    },
    ClientSendCommand {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "command-id")]
        command_id: String,
        #[arg(long = "session-id")]
        session_id: String,
        #[arg(long, value_enum)]
        kind: DaemonCommandKindArg,
        #[arg(long = "access-mode", value_enum, default_value = "read-only")]
        access_mode: ClientAccessModeArg,
    },
    ClientReadResponse {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "command-id")]
        command_id: String,
    },
    ClientListSessions {
        #[arg(long)]
        path: PathBuf,
    },
    ClientInspectSession {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "session-id")]
        session_id: String,
        #[arg(long, value_enum, default_value = "raw")]
        mode: SessionExportModeArg,
    },
    ClientExportSession {
        #[arg(long)]
        path: PathBuf,
        #[arg(long = "session-id")]
        session_id: String,
    },
}

fn sample_request(scenario: Scenario) -> ReasoningHarnessRequest {
    let mut request = ReasoningHarnessRequest {
        objective: "continue a long-running coding task".to_string(),
        constraints: vec![
            "do not spin".to_string(),
            "checkpoint before handoff".to_string(),
        ],
        evidence_scope: vec!["current repository".to_string()],
        allowed_capability_classes: vec!["tool".to_string(), "skill".to_string()],
        prohibited_behaviors: vec![
            ProhibitionRule::NoFakeProgress,
            ProhibitionRule::NoRepeatedNoProgressContinuation,
            ProhibitionRule::NoDuplicateInjection,
        ],
        progress: ProgressSnapshot {
            completed_steps: 3,
            total_steps_hint: Some(8),
            last_meaningful_progress_at_ms: Some(1_700_000_000_000),
            no_progress_iteration_count: 1,
        },
        recovery: RecoverySnapshot {
            last_recovery_reason: Some(RecoveryReason::PartialResult),
            recovery_attempt_count: 1,
            last_recovery_had_progress: true,
        },
        last_checkpoint: Some(CheckpointRef {
            checkpoint_id: "cp-1".to_string(),
            thread_id: "thread-1".to_string(),
            turn_id: "turn-5".to_string(),
            created_at_ms: 1_700_000_000_000,
            summary: "checkpoint after media analysis".to_string(),
        }),
        self_correction: SelfCorrectionState {
            active: false,
            correction_attempt_count: 0,
            last_correction_target: None,
            last_correction_had_progress: false,
        },
        post_compaction: PostCompactionState {
            pending_post_compaction: false,
            first_post_compaction_turn: false,
            compaction_window_id: Some("window-1".to_string()),
            last_compaction_checkpoint_id: Some("cp-1".to_string()),
        },
        runtime_mode: RuntimeModeState {
            active_agent_mode: Some("build".to_string()),
            previous_agent_mode: Some("plan".to_string()),
            active_execution_mode: Some(ExecutionMode::Interactive),
            previous_execution_mode: Some(ExecutionMode::Resumable),
            mode_transition_pending_guidance: false,
            invoked_skills: vec![SkillInvocationState {
                skill_id: "context-budget-gate".to_string(),
                invocation_ref: Some("skill://gate/1".to_string()),
                invoked_at_ms: Some(1_700_000_000_123),
            }],
        },
        pending_approval: PendingApprovalState {
            active_request: None,
            recent_decision: Some(PermissionDecision {
                request_id: "approval-0".to_string(),
                status: PermissionDecisionStatus::Approved,
                decided_at_ms: Some(1_699_999_999_500),
                decided_by: Some("operator".to_string()),
                note: Some("continue with local verification only".to_string()),
            }),
            blocked_on_approval: false,
            may_continue_other_work: true,
        },
        interrupts: InterruptQueueState {
            pending_interrupts: vec![PendingInterrupt {
                interrupt_id: "interrupt-1".to_string(),
                kind: InterruptKind::SystemNotification,
                summary: "new runtime guidance is ready at the next safe point".to_string(),
                injection_mode: InterruptInjectionMode::ImmediateSafePoint,
                created_at_ms: Some(1_700_000_000_150),
            }],
            safe_point_requested: true,
            last_interrupt_at_ms: Some(1_700_000_000_150),
        },
        last_run_report: Some(RunReport {
            run_id: "run-healthy-1".to_string(),
            thread_id: "thread-1".to_string(),
            objective: "continue a long-running coding task".to_string(),
            execution_mode: ExecutionMode::Interactive,
            outcome: RunOutcome::InProgress,
            rationale: "carry validated state into the next cycle".to_string(),
            cycle_summary: Some(CycleSummary {
                cycle_id: "cycle-1".to_string(),
                summary: "validated recent progress and left a safe-point interrupt queued".to_string(),
                completed_steps_delta: 1,
                state_changes: vec![muldex_core::protocol::StateChangeKind::NewConfirmedFinding],
                checkpoint_created: false,
                approval_request_id: None,
                pending_interrupt_count: 1,
            }),
            generated_at_ms: Some(1_700_000_000_175),
        }),
        safety: PermissionContextSnapshot {
            sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
            approval_policy: ApprovalPolicyDescriptor::OnRequest,
            permission_profile_summary: "managed".to_string(),
            network_access_enabled: false,
            requires_explicit_approval_for_next_step: false,
        },
        codex_continuation: Some(CodexSessionContinuationSnapshot {
            source_thread_id: "thread-1".to_string(),
            source_turn_id: "turn-5".to_string(),
            source_model: "gpt-5.4".to_string(),
            source_provider: "llm-router".to_string(),
            active_agent_mode: Some("build".to_string()),
            safety: PermissionContextSnapshot {
                sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                approval_policy: ApprovalPolicyDescriptor::OnRequest,
                permission_profile_summary: "managed".to_string(),
                network_access_enabled: false,
                requires_explicit_approval_for_next_step: false,
            },
            reference_context_present: true,
        }),
        context_pressure: ContextPressure {
            model_context_window: Some(256_000),
            active_context_tokens: Some(140_000),
            auto_compact_scope_tokens: Some(24_000),
            auto_compact_limit: Some(192_000),
            tokens_until_compaction: Some(52_000),
            recent_compaction_count: 1,
            last_compaction_had_state_change: true,
        },
        media_context: Vec::<MediaContextEnvelope>::new(),
        capability_registry: CapabilityRegistrySnapshot::default(),
        escalation_policy: EscalationPolicy {
            no_progress_limit: 3,
            repeated_compaction_limit: 2,
            self_correction_limit: 2,
            request_checkpoint_before_handoff: true,
        },
    };

    match scenario {
        Scenario::Healthy => {}
        Scenario::NoProgress => {
            request.progress.no_progress_iteration_count = 3;
            request.recovery.last_recovery_had_progress = false;
            request.last_run_report = Some(RunReport {
                run_id: "run-no-progress-1".to_string(),
                thread_id: "thread-1".to_string(),
                objective: request.objective.clone(),
                execution_mode: ExecutionMode::Interactive,
                outcome: RunOutcome::InProgress,
                rationale: "progress stalled across repeated iterations".to_string(),
                cycle_summary: Some(CycleSummary {
                    cycle_id: "cycle-stall-1".to_string(),
                    summary: "no meaningful state change across the last cycle".to_string(),
                    completed_steps_delta: 0,
                    state_changes: vec![muldex_core::protocol::StateChangeKind::NoMeaningfulChange],
                    checkpoint_created: false,
                    approval_request_id: None,
                    pending_interrupt_count: 1,
                }),
                generated_at_ms: Some(1_700_000_001_000),
            });
        }
        Scenario::RecoverableFailure => {
            request.recovery.last_recovery_reason = Some(RecoveryReason::ToolFailure);
            request.recovery.last_recovery_had_progress = false;
            request.self_correction.active = true;
            request.self_correction.correction_attempt_count = 1;
            request.self_correction.last_correction_target = Some("retry failed tool step".to_string());
            request.pending_approval.active_request = Some(PermissionRequest {
                request_id: "approval-tool-retry".to_string(),
                action_kind: PermissionActionKind::ShellExecution,
                summary: "rerun a failing tool step with expanded permissions".to_string(),
                rationale: "recovery path may need a broader shell invocation".to_string(),
                urgency: PermissionUrgency::Normal,
                wait_for_decision: false,
                requested_at_ms: Some(1_700_000_001_100),
                expires_at_ms: None,
            });
        }
        Scenario::PostCompactionStall => {
            request.post_compaction.pending_post_compaction = true;
            request.post_compaction.first_post_compaction_turn = true;
            request.progress.no_progress_iteration_count = 2;
            request.recovery.last_recovery_had_progress = false;
            request.runtime_mode.previous_execution_mode = Some(ExecutionMode::Scheduled);
        }
        Scenario::MediaHeavy => {
            request.objective = "analyze multimodal evidence and continue safely".to_string();
            request.runtime_mode.active_execution_mode = Some(ExecutionMode::Streaming);
            request.media_context.push(MediaContextEnvelope {
                asset: crate_media_asset("video-1", "clips/demo.mp4"),
                derived_artifacts: Vec::new(),
                hyperframes: Vec::new(),
                operator_summary: "video and transcript are available".to_string(),
                model_summary: "use hyperframe-aligned evidence".to_string(),
                token_budget_hint: Some(4096),
            });
        }
    }

    request
}

fn crate_media_asset(asset_id: &str, path: &str) -> MediaAssetRef {
    MediaAssetRef {
        asset_id: asset_id.to_string(),
        kind: MediaKind::Video,
        source: MediaSource::LocalPath {
            path: path.to_string(),
        },
        display_name: Some(asset_id.to_string()),
    }
}

fn print_decision(decision: &muldex_core::reasoning_harness::ReasoningHarnessDecision) {
    println!("mode: {}", match decision.mode {
        ContinueMode::SameTurn => "same_turn",
        ContinueMode::NextTurn => "next_turn",
        ContinueMode::QueueOnly => "queue_only",
        ContinueMode::Handoff => "handoff",
        ContinueMode::Stop => "stop",
    });
    println!("checkpoint: {}", decision.should_checkpoint);
    println!("self_correction: {}", decision.should_enter_self_correction);
    println!("pause_for_approval: {}", decision.pause_for_approval);
    println!("consume_interrupts_now: {}", decision.consume_interrupts_now);
    println!("may_continue_other_work: {}", decision.may_continue_other_work);
    println!("rationale: {}", decision.rationale);
    if !decision.violated_rules.is_empty() {
        println!("violated_rules: {:?}", decision.violated_rules);
    }
}

fn print_runtime_step_result(result: &RuntimeStepResult) {
    println!("runtime.cycle_index: {}", result.updated_state.cycle_index);
    println!("runtime.phase: {:?}", result.updated_state.phase);
    println!("runtime.outcome: {:?}", result.report.outcome);
    println!(
        "runtime.consumed_interrupts: {}",
        result.consumed_interrupts.len()
    );
    if let Some(summary) = &result.report.cycle_summary {
        println!("runtime.cycle_id: {}", summary.cycle_id);
        println!("runtime.cycle_summary: {}", summary.summary);
        println!(
            "runtime.pending_interrupt_count: {}",
            summary.pending_interrupt_count
        );
    }
}

fn print_runtime_drive_result(result: &RuntimeDriveResult) {
    println!("runtime.steps: {}", result.step_results.len());
    println!("runtime.final_phase: {:?}", result.final_state.phase);
    println!("runtime.final_cycle_index: {}", result.final_state.cycle_index);
    if let Some(report) = &result.final_state.latest_report {
        println!("runtime.final_outcome: {:?}", report.outcome);
        println!("runtime.final_rationale: {}", report.rationale);
    }
}

fn print_runtime_state_summary(state: &RuntimeState) {
    println!("runtime.thread_id: {}", state.request.thread_id);
    println!("runtime.turn_id: {}", state.request.turn_id);
    println!("runtime.phase: {:?}", state.phase);
    println!("runtime.cycle_index: {}", state.cycle_index);
    println!(
        "runtime.execution_mode: {:?}",
        state.request.runtime_mode.active_execution_mode
    );
    println!("runtime.objective: {}", state.request.objective);
}

fn print_host_summary(host: &RuntimeHost) {
    let sessions = host.list_sessions();
    println!("host.session_count: {}", sessions.len());
    for session in sessions {
        println!(
            "host.session: {} thread={} phase={:?} cycle={}",
            session.session_id, session.thread_id, session.phase, session.cycle_index
        );
    }
}

fn print_daemon_summary(daemon: &RuntimeDaemon) {
    println!("daemon.snapshot_path: {}", daemon.snapshot_path().display());
    println!("daemon.status: {:?}", daemon.status());
    if let Ok(host) = daemon.host() {
        print_host_summary(host);
    }
}

fn runtime_state_from_request(request: ReasoningHarnessRequest) -> RuntimeState {
    RuntimeState {
        request: ContinueRequest {
            thread_id: request
                .codex_continuation
                .as_ref()
                .map(|snapshot| snapshot.source_thread_id.clone())
                .unwrap_or_else(|| "thread-1".to_string()),
            turn_id: request
                .codex_continuation
                .as_ref()
                .map(|snapshot| snapshot.source_turn_id.clone())
                .unwrap_or_else(|| "turn-1".to_string()),
            objective: request.objective,
            constraints: request.constraints,
            continue_reason: ContinueReason::ManualUserRequest,
            recent_state_changes: vec![muldex_core::protocol::StateChangeKind::UserDecision],
            working_hypothesis: None,
            last_agent_message: None,
            pending_input_count: 0,
            trigger_turn_pending: false,
            tool_call_count_this_turn: 0,
            context_pressure: request.context_pressure,
            duplicate_injection_detected: false,
            repeated_follow_up_count: 0,
            progress: request.progress,
            recovery: request.recovery,
            last_checkpoint: request.last_checkpoint,
            self_correction: request.self_correction,
            post_compaction: request.post_compaction,
            runtime_mode: request.runtime_mode,
            pending_approval: request.pending_approval,
            interrupts: request.interrupts,
            last_run_report: request.last_run_report,
            safety: request.safety,
            codex_continuation: request.codex_continuation,
        },
        cycle_index: 0,
        phase: RuntimePhase::Ready,
        latest_report: None,
    }
}

fn apply_runtime_step(request: ReasoningHarnessRequest) -> RuntimeStepResult {
    let state = runtime_state_from_request(request);
    let mut driver = RuntimeDriver::new(state);
    let harness_decision = decide_reasoning_harness(&ReasoningHarnessRequest {
        objective: driver.state.request.objective.clone(),
        constraints: driver.state.request.constraints.clone(),
        evidence_scope: Vec::new(),
        allowed_capability_classes: Vec::new(),
        prohibited_behaviors: vec![],
        progress: driver.state.request.progress.clone(),
        recovery: driver.state.request.recovery.clone(),
        last_checkpoint: driver.state.request.last_checkpoint.clone(),
        self_correction: driver.state.request.self_correction.clone(),
        post_compaction: driver.state.request.post_compaction.clone(),
        runtime_mode: driver.state.request.runtime_mode.clone(),
        pending_approval: driver.state.request.pending_approval.clone(),
        interrupts: driver.state.request.interrupts.clone(),
        last_run_report: driver.state.request.last_run_report.clone(),
        safety: driver.state.request.safety.clone(),
        codex_continuation: driver.state.request.codex_continuation.clone(),
        context_pressure: driver.state.request.context_pressure.clone(),
        media_context: Vec::new(),
        capability_registry: CapabilityRegistrySnapshot::default(),
        escalation_policy: EscalationPolicy {
            no_progress_limit: 3,
            repeated_compaction_limit: 2,
            self_correction_limit: 2,
            request_checkpoint_before_handoff: true,
        },
    });

    match driver.apply_command(RuntimeCommand::Decision(ContinueDecision {
        allow_continue: !matches!(harness_decision.mode, ContinueMode::Handoff | ContinueMode::Stop),
        mode: harness_decision.mode.clone(),
        rationale: harness_decision.rationale.clone(),
        next_action: None,
        pause_for_approval: harness_decision.pause_for_approval,
        consume_interrupts_now: harness_decision.consume_interrupts_now,
        may_continue_other_work: harness_decision.may_continue_other_work,
        suppress_duplicate_injection: false,
        downgrade_trigger_turn: false,
        request_compaction: false,
        request_handoff_summary: matches!(harness_decision.mode, ContinueMode::Handoff),
        request_checkpoint: harness_decision.should_checkpoint,
        enter_self_correction: harness_decision.should_enter_self_correction,
    })) {
        RuntimeCommandResult::Step(result) => result,
        other => panic!("unexpected runtime command result for decision: {:?}", other),
    }
}

fn demo_approval_resume() {
    let mut request = sample_request(Scenario::Healthy);
    request.pending_approval.active_request = Some(PermissionRequest {
        request_id: "approval-demo-1".to_string(),
        action_kind: PermissionActionKind::RemoteMutation,
        summary: "open a pull request for review".to_string(),
        rationale: "the next step would publish code outside the local runtime".to_string(),
        urgency: PermissionUrgency::Normal,
        wait_for_decision: false,
        requested_at_ms: Some(1_700_000_010_000),
        expires_at_ms: None,
    });
    request.pending_approval.blocked_on_approval = true;
    request.pending_approval.may_continue_other_work = true;

    println!("demo.phase: waiting for approval");
    let initial_state = runtime_state_from_request(request);
    let mut driver = RuntimeDriver::new(initial_state);
    let waiting = match driver.apply_command(RuntimeCommand::Drive {
        decisions: vec![ContinueDecision {
            allow_continue: false,
            mode: ContinueMode::QueueOnly,
            rationale: "pause risky work until approval arrives".to_string(),
            next_action: None,
            pause_for_approval: true,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        }],
        cycle_limit: 4,
    }) {
        RuntimeCommandResult::Drive(result) => result,
        other => panic!("unexpected runtime command result for drive: {:?}", other),
    };
    print_runtime_drive_result(&waiting);
    println!();

    println!("demo.phase: approval event arrives and runtime resumes");
    let resumed = match driver.apply_command(RuntimeCommand::ResumeAfterEvent {
        event: RuntimeEvent::RecordApprovalDecision(PermissionDecision {
            request_id: "approval-demo-1".to_string(),
            status: PermissionDecisionStatus::Approved,
            decided_at_ms: Some(1_700_000_010_500),
            decided_by: Some("operator".to_string()),
            note: Some("continue with bounded progress".to_string()),
        }),
        decisions: vec![ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::NextTurn,
            rationale: "approval cleared, resume bounded progress".to_string(),
            next_action: Some("continue verification".to_string()),
            pause_for_approval: false,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        }],
        cycle_limit: 4,
    }) {
        RuntimeCommandResult::Resume(result) => result,
        other => panic!("unexpected runtime command result for resume: {:?}", other),
    };
    println!("runtime.resumed_phase: {:?}", resumed.resumed_state.phase);
    print_runtime_drive_result(&resumed.drive_result);
}

fn demo_host_persistence() -> Result<(), Box<dyn std::error::Error>> {
    let request = sample_request(Scenario::Healthy);
    let state = runtime_state_from_request(request);
    let mut host = RuntimeHost::new();
    host.create_session("demo-session", state)?;

    let snapshot_path = std::env::temp_dir().join("muldex-demo-host-snapshot.json");
    host.save_snapshot_to_path(&snapshot_path)?;
    println!("demo.snapshot_saved: {}", snapshot_path.display());

    let mut restored = RuntimeHost::load_snapshot_from_path(&snapshot_path)?;
    let result = restored.apply_command(
        "demo-session",
        RuntimeCommand::Decision(ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::NextTurn,
            rationale: "resume from restored host state".to_string(),
            next_action: Some("continue verification".to_string()),
            pause_for_approval: false,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        }),
    )?;

    match result {
        RuntimeCommandResult::Step(step) => {
            println!("demo.restored_phase: {:?}", step.updated_state.phase);
            println!("demo.restored_cycle_index: {}", step.updated_state.cycle_index);
            println!("demo.restored_outcome: {:?}", step.report.outcome);
        }
        other => {
            return Err(format!("unexpected runtime result after restore: {:?}", other).into());
        }
    }

    let _ = std::fs::remove_file(&snapshot_path);
    Ok(())
}

fn save_host_snapshot(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let request = sample_request(Scenario::Healthy);
    let state = runtime_state_from_request(request);
    let mut host = RuntimeHost::new();
    host.create_session("sample-session", state)?;
    host.save_snapshot_to_path(&path)?;

    let snapshot = export_host(&host);
    println!("host.snapshot_path: {}", path.display());
    println!("host.snapshot_sessions: {}", snapshot.sessions.len());
    print_host_summary(&host);
    Ok(())
}

fn load_host_snapshot(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let host = RuntimeHost::load_snapshot_from_path(&path)?;
    println!("host.snapshot_path: {}", path.display());
    print_host_summary(&host);
    Ok(())
}

fn import_codex_snapshot(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(path)?;
    let state = match serde_json::from_str::<CodexSignalSnapshot>(&raw) {
        Ok(snapshot) => import_external_snapshot_as_runtime_state(
            ExternalRuntimeSnapshot::CodexSignal(snapshot),
        ),
        Err(_) => match serde_json::from_str::<CodexLiveContinuationSnapshot>(&raw) {
            Ok(snapshot) => import_external_snapshot_as_runtime_state(
                ExternalRuntimeSnapshot::CodexLive(snapshot),
            ),
            Err(_) => {
                let snapshot: CodexBootstrapSnapshot = serde_json::from_str(&raw)?;
                import_external_snapshot_as_runtime_state(
                    ExternalRuntimeSnapshot::CodexBootstrap(snapshot),
                )
            }
        },
    };

    print_runtime_state_summary(&state);
    Ok(())
}

fn export_session_view_command(
    path: PathBuf,
    session_id: String,
    mode: SessionExportModeArg,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = RuntimeHost::load_snapshot_from_path(&path)?;
    let mode = match mode {
        SessionExportModeArg::Raw => ReportExportMode::Raw,
        SessionExportModeArg::Compressed => {
            let previous = muldex_runtime::continuity::export_latest_report_raw(&host, &session_id)?;
            let view = export_session_view(
                &host,
                &session_id,
                ReportExportMode::Compressed,
                previous.as_ref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&view)?);
            return Ok(());
        }
    };

    let view = export_session_view(&host, &session_id, mode, None)?;
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

fn daemon_boot_empty(path: PathBuf) {
    let mut daemon = RuntimeDaemon::new(path);
    daemon.boot_empty().expect("boot empty daemon");
    print_daemon_summary(&daemon);
}

fn daemon_boot_load(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = RuntimeDaemon::new(path);
    daemon.boot_from_disk_if_present()?;
    print_daemon_summary(&daemon);
    Ok(())
}

fn daemon_save(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = RuntimeDaemon::new(&path);
    daemon.boot_from_disk_if_present()?;
    daemon.save()?;
    print_daemon_summary(&daemon);
    Ok(())
}

fn daemon_status(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = RuntimeDaemon::new(&path);
    daemon.boot_from_disk_if_present()?;
    print_daemon_summary(&daemon);
    Ok(())
}

fn daemon_serve_once(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = RuntimeDaemon::new(&path);
    daemon.boot_from_disk_if_present()?;
    let responses = daemon.serve_once()?;
    println!("daemon.responses: {}", responses.len());
    daemon.shutdown()?;
    Ok(())
}

fn daemon_serve_loop(path: PathBuf, iterations: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = RuntimeDaemon::new(&path);
    daemon.boot_from_disk_if_present()?;
    let result = daemon.serve_loop(iterations)?;
    println!("daemon.iterations: {}", result.iterations);
    println!("daemon.total_responses: {}", result.total_responses);
    daemon.shutdown()?;
    Ok(())
}

fn transport_root_for_snapshot(path: &PathBuf) -> PathBuf {
    match (path.parent(), path.file_stem().and_then(|stem| stem.to_str())) {
        (Some(parent), Some(stem)) => parent.join(format!("{stem}.muldex-transport")),
        (Some(parent), None) => parent.join(".muldex-transport"),
        (None, Some(stem)) => PathBuf::from(format!("{stem}.muldex-transport")),
        (None, None) => PathBuf::from(".muldex-transport"),
    }
}

fn daemon_send_command(
    path: PathBuf,
    command_id: String,
    session_id: String,
    kind: DaemonCommandKindArg,
) -> Result<(), Box<dyn std::error::Error>> {
    let command = match kind {
        DaemonCommandKindArg::Status => RuntimeCommand::Decision(ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::QueueOnly,
            rationale: "status probe through daemon transport".to_string(),
            next_action: None,
            pause_for_approval: false,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        }),
        DaemonCommandKindArg::AdvanceSample => RuntimeCommand::Decision(ContinueDecision {
            allow_continue: true,
            mode: ContinueMode::NextTurn,
            rationale: "advance sample through daemon transport".to_string(),
            next_action: Some("continue sample work".to_string()),
            pause_for_approval: false,
            consume_interrupts_now: false,
            may_continue_other_work: true,
            suppress_duplicate_injection: false,
            downgrade_trigger_turn: false,
            request_compaction: false,
            request_handoff_summary: false,
            request_checkpoint: false,
            enter_self_correction: false,
        }),
    };

    let transport = FileCommandTransport::new(transport_root_for_snapshot(&path));
    let envelope = DaemonCommandEnvelope {
        schema_version: "daemon-envelope-v1".to_string(),
        command_id,
        session_id: Some(session_id),
        command_name: "apply_command".to_string(),
        payload_kind: "RuntimeCommand".to_string(),
        payload_json: serde_json::to_string_pretty(&command)?,
        created_at_ms: 0,
    };
    let command_path = transport.write_command(&envelope)?;
    println!("daemon.command_path: {}", command_path.display());
    Ok(())
}

fn daemon_read_response(
    path: PathBuf,
    command_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let transport = FileCommandTransport::new(transport_root_for_snapshot(&path));
    let response = transport.read_response(&command_id)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

fn daemon_stale_status(path: PathBuf, threshold_ms: u64) -> Result<(), Box<dyn std::error::Error>> {
    let daemon = RuntimeDaemon::new(&path);
    let report = daemon.stale_report(threshold_ms)?;
    match report.status {
        StaleOwnershipStatus::NoLock => {
            println!("daemon.stale_status: no_lock");
        }
        StaleOwnershipStatus::Fresh { heartbeat_age_ms } => {
            println!("daemon.stale_status: fresh");
            println!("daemon.heartbeat_age_ms: {}", heartbeat_age_ms);
        }
        StaleOwnershipStatus::Stale { heartbeat_age_ms } => {
            println!("daemon.stale_status: stale");
            println!("daemon.heartbeat_age_ms: {}", heartbeat_age_ms);
        }
    }
    println!("daemon.threshold_ms: {}", report.stale_threshold_ms);
    if let Some(lock) = report.lock {
        println!("daemon.owner_pid: {}", lock.owner_pid);
        println!("daemon.lock_created_at_ms: {}", lock.created_at_ms);
        println!("daemon.last_heartbeat_ms: {}", lock.last_heartbeat_ms);
    }
    Ok(())
}

fn daemon_force_takeover(path: PathBuf, threshold_ms: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = RuntimeDaemon::new(&path);
    daemon.force_takeover(threshold_ms)?;
    println!("daemon.force_takeover: ok");
    let report = daemon.stale_report(threshold_ms)?;
    if let Some(lock) = report.lock {
        println!("daemon.owner_pid: {}", lock.owner_pid);
        println!("daemon.last_heartbeat_ms: {}", lock.last_heartbeat_ms);
    }
    Ok(())
}

fn server_foreground(path: PathBuf, iterations: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = RuntimeDaemon::new(&path);
    daemon.boot_from_disk_if_present()?;
    let result = daemon.serve_loop(iterations)?;
    println!("server.iterations: {}", result.iterations);
    println!("server.total_responses: {}", result.total_responses);
    daemon.shutdown()?;
    Ok(())
}

fn client_status(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let daemon = RuntimeDaemon::new(&path);
    let stale = daemon.stale_report(60_000)?;
    let view = daemon_status_view(&daemon, Some(stale.status));
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

fn client_send_command(
    path: PathBuf,
    command_id: String,
    session_id: String,
    kind: DaemonCommandKindArg,
    access_mode: ClientAccessModeArg,
) -> Result<(), Box<dyn std::error::Error>> {
    let access_mode = match access_mode {
        ClientAccessModeArg::ReadOnly => ClientAccessMode::ReadOnly,
        ClientAccessModeArg::Full => ClientAccessMode::Full,
    };
    let client_command = match kind {
        DaemonCommandKindArg::Status => ClientCommandView::Status,
        DaemonCommandKindArg::AdvanceSample => ClientCommandView::AdvanceSample,
    };
    let kind_name = match client_command {
        ClientCommandView::Status => "status",
        ClientCommandView::AdvanceSample => "advance-sample",
    };
    if !client_command_allowed(&access_mode, kind_name) {
        return Err(format!(
            "client access mode {:?} does not allow command kind {}",
            access_mode, kind_name
        )
        .into());
    }

    let command_view = command_envelope_view(
        session_id.clone(),
        access_mode.clone(),
        client_command.clone(),
    );
    let runtime_command = project_client_command(&command_view.command);
    let command_name = "apply_command".to_string();
    let transport = FileCommandTransport::new(transport_root_for_snapshot(&path));
    let envelope = DaemonCommandEnvelope {
        schema_version: "daemon-envelope-v1".to_string(),
        command_id: command_id.clone(),
        session_id: Some(session_id.clone()),
        command_name: command_name.clone(),
        payload_kind: "RuntimeCommand".to_string(),
        payload_json: serde_json::to_string_pretty(&runtime_command)?,
        created_at_ms: 0,
    };
    let command_path = transport.write_command(&envelope)?;
    let view = command_receipt_view(
        command_id,
        Some(session_id),
        command_name,
        command_path.display().to_string(),
    );
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

fn client_read_response(
    path: PathBuf,
    command_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let transport = FileCommandTransport::new(transport_root_for_snapshot(&path));
    let response = transport.read_response(&command_id)?;
    let view = response_view(response);
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

fn client_list_sessions(path: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let host = RuntimeHost::load_snapshot_from_path(&path)?;
    let view = session_list_view(&host);
    println!("{}", serde_json::to_string_pretty(&view)?);
    Ok(())
}

#[cfg(test)]
fn load_client_session_list_view(path: PathBuf) -> Result<muldex_runtime::client_views::ClientSessionListView, Box<dyn std::error::Error>> {
    let host = RuntimeHost::load_snapshot_from_path(&path)?;
    Ok(session_list_view(&host))
}

fn client_inspect_session(
    path: PathBuf,
    session_id: String,
    mode: SessionExportModeArg,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = RuntimeHost::load_snapshot_from_path(&path)?;
    let mode = match mode {
        SessionExportModeArg::Raw => ReportExportMode::Raw,
        SessionExportModeArg::Compressed => {
            let previous = muldex_runtime::continuity::export_latest_report_raw(&host, &session_id)?;
            let view = export_session_view(
                &host,
                &session_id,
                ReportExportMode::Compressed,
                previous.as_ref(),
            )?;
            let normalized = inspect_session_view(view);
            println!("{}", serde_json::to_string_pretty(&normalized)?);
            return Ok(());
        }
    };

    let view = export_session_view(&host, &session_id, mode, None)?;
    let normalized = inspect_session_view(view);
    println!("{}", serde_json::to_string_pretty(&normalized)?);
    Ok(())
}

#[cfg(test)]
fn load_client_inspect_session_view(
    path: PathBuf,
    session_id: String,
    mode: SessionExportModeArg,
) -> Result<muldex_runtime::continuity::ExportedSessionView, Box<dyn std::error::Error>> {
    let host = RuntimeHost::load_snapshot_from_path(&path)?;
    let mode = match mode {
        SessionExportModeArg::Raw => ReportExportMode::Raw,
        SessionExportModeArg::Compressed => ReportExportMode::Compressed,
    };
    let previous_report = if matches!(mode, ReportExportMode::Compressed) {
        muldex_runtime::continuity::export_latest_report_raw(&host, &session_id)?
    } else {
        None
    };
    let view = export_session_view(&host, &session_id, mode, previous_report.as_ref())?;
    Ok(inspect_session_view(view))
}

fn client_export_session(
    path: PathBuf,
    session_id: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = RuntimeHost::load_snapshot_from_path(&path)?;
    let snapshot = export_session(&host, &session_id)?;
    println!("{}", serde_json::to_string_pretty(&snapshot)?);
    Ok(())
}

fn parse_interactive_shell_input(line: &str) -> InteractiveShellInput {
    let normalized = line.trim_end_matches(['\r', '\n']);
    let trimmed = normalized.trim();
    let single_line = !normalized.contains('\n');
    if trimmed.is_empty() {
        InteractiveShellInput::Empty
    } else if matches!(trimmed, "/exit" | "/quit") {
        InteractiveShellInput::Exit
    } else if trimmed == "/help" {
        InteractiveShellInput::Help
    } else if trimmed == "/status" {
        InteractiveShellInput::Status
    } else if single_line && trimmed.starts_with('/') {
        InteractiveShellInput::SlashCommand(parse_interactive_slash_command(trimmed))
    } else {
        InteractiveShellInput::Prompt(trimmed.to_string())
    }
}

fn parse_interactive_slash_command(command: &str) -> InteractiveSlashCommand {
    let mut parts = command.split_whitespace();
    match parts.next() {
        Some("/model") => InteractiveSlashCommand::Model(parts.next().map(str::to_string)),
        Some("/approval") => InteractiveSlashCommand::Approval(parts.next().map(str::to_string)),
        Some("/compact") => InteractiveSlashCommand::Compact,
        Some("/sessions") => InteractiveSlashCommand::Sessions,
        Some("/resume") => InteractiveSlashCommand::Resume(parts.next().map(str::to_string)),
        Some("/new") => InteractiveSlashCommand::New,
        Some("/config") => {
            match parts.next() {
                Some("llm") | None => InteractiveSlashCommand::ConfigLlm(parse_interactive_llm_config_command(parts.collect())),
                Some(other) => InteractiveSlashCommand::Unknown(format!("/config {other}")),
            }
        }
        Some("/provider") => InteractiveSlashCommand::Provider(parse_interactive_provider_command(parts.collect())),
        Some(other) => InteractiveSlashCommand::Unknown(other.to_string()),
        None => InteractiveSlashCommand::Unknown(command.to_string()),
    }
}

fn parse_interactive_llm_config_command(parts: Vec<&str>) -> InteractiveLlmConfigCommand {
    match parts.as_slice() {
        [] | ["show"] => InteractiveLlmConfigCommand::Show,
        ["test"] => InteractiveLlmConfigCommand::Test,
        ["host", value] => InteractiveLlmConfigCommand::SetHost((*value).to_string()),
        ["port", value] => match value.parse::<u16>() {
            Ok(port) => InteractiveLlmConfigCommand::SetPort(port),
            Err(_) => InteractiveLlmConfigCommand::Invalid("invalid port".to_string()),
        },
        ["api-key", value] => InteractiveLlmConfigCommand::SetApiKey((*value).to_string()),
        ["default-model", value] => {
            InteractiveLlmConfigCommand::SetDefaultModel((*value).to_string())
        }
        _ => InteractiveLlmConfigCommand::Invalid("unsupported llm config command".to_string()),
    }
}

fn parse_interactive_provider_command(parts: Vec<&str>) -> InteractiveProviderCommand {
    match parts.as_slice() {
        [] | ["show"] => InteractiveProviderCommand::Show,
        ["list"] => InteractiveProviderCommand::List,
        ["test"] => InteractiveProviderCommand::Test(None),
        ["test", value] => InteractiveProviderCommand::Test(Some((*value).to_string())),
        ["use", value] => InteractiveProviderCommand::Use((*value).to_string()),
        _ => InteractiveProviderCommand::Invalid("unsupported provider command".to_string()),
    }
}

fn interactive_shell_driver() -> RuntimeDriver {
    let mut request = sample_request(Scenario::Healthy);
    request.objective = "interactive muldex shell".to_string();
    RuntimeDriver::new(runtime_state_from_request(request))
}

fn interactive_shell_state() -> InteractiveShellState {
    InteractiveShellState {
        model: "gpt-5.4".to_string(),
        approval_mode: "on-request".to_string(),
        compact_count: 0,
        resume_count: 0,
    }
}

fn runtime_model_label(state: &RuntimeState, shell: &InteractiveShellState) -> String {
    state
        .request
        .codex_continuation
        .as_ref()
        .map(|snapshot| snapshot.source_model.clone())
        .unwrap_or_else(|| shell.model.clone())
}

fn ensure_runtime_model_state(state: &mut RuntimeState, shell: &InteractiveShellState) {
    if state.request.codex_continuation.is_none() {
        state.request.codex_continuation = Some(CodexSessionContinuationSnapshot {
            source_thread_id: state.request.thread_id.clone(),
            source_turn_id: state.request.turn_id.clone(),
            source_model: shell.model.clone(),
            source_provider: "interactive-shell".to_string(),
            active_agent_mode: state.request.runtime_mode.active_agent_mode.clone(),
            safety: state.request.safety.clone(),
            reference_context_present: true,
        });
    }
}

fn parse_approval_policy(input: &str) -> Option<ApprovalPolicyDescriptor> {
    match input {
        "ask" | "manual" => Some(ApprovalPolicyDescriptor::Ask),
        "on-request" => Some(ApprovalPolicyDescriptor::OnRequest),
        "never" | "auto" => Some(ApprovalPolicyDescriptor::Never),
        "unless-trusted" => Some(ApprovalPolicyDescriptor::UnlessTrusted),
        _ => None,
    }
}

fn approval_policy_label(policy: &ApprovalPolicyDescriptor) -> &'static str {
    match policy {
        ApprovalPolicyDescriptor::Never => "never",
        ApprovalPolicyDescriptor::Ask => "ask",
        ApprovalPolicyDescriptor::OnRequest => "on-request",
        ApprovalPolicyDescriptor::UnlessTrusted => "unless-trusted",
        ApprovalPolicyDescriptor::Unknown => "unknown",
    }
}

fn interactive_shell_session_id() -> String {
    format!(
        "interactive-session-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0)
    )
}

fn interactive_shell_snapshot_path() -> PathBuf {
    match std::env::var("MULDEX_INTERACTIVE_SHELL_PATH") {
        Ok(path) => PathBuf::from(path),
        Err(_) => std::env::temp_dir().join("muldex-interactive-shell.json"),
    }
}

fn muldex_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("MULDEX_CONFIG_PATH") {
        return PathBuf::from(path);
    }
    if cfg!(windows) {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".muldex").join("config.json")
    } else {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".muldex").join("config.json")
    }
}

fn load_muldex_config() -> Result<MuldexConfig, Box<dyn std::error::Error>> {
    let path = muldex_config_path();
    if !path.exists() {
        return Ok(MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("llm-router".to_string()),
            providers: std::collections::BTreeMap::new(),
            llm_router: None,
        });
    }
    let raw = fs::read_to_string(path)?;
    let mut config: MuldexConfig = serde_json::from_str(&raw)?;
    if config.default_provider.is_none() {
        config.default_provider = Some("llm-router".to_string());
    }
    if config.providers.is_empty() {
        if let Some(legacy) = config.llm_router.as_ref() {
            config.providers.insert(
                "llm-router".to_string(),
                ProviderConfig {
                    kind: "openai-compatible".to_string(),
                    host: Some(legacy.host.clone()),
                    port: Some(legacy.port),
                    base_url: None,
                    api_key: Some(legacy.api_key.clone()),
                    api_key_env: None,
                    default_model: legacy.default_model.clone(),
                },
            );
        }
    }
    Ok(config)
}

fn save_muldex_config(config: &MuldexConfig) -> Result<(), Box<dyn std::error::Error>> {
    let path = muldex_config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

fn masked_api_key(api_key: &str) -> String {
    if api_key.is_empty() {
        return "not-set".to_string();
    }
    let suffix = api_key.chars().rev().take(4).collect::<String>().chars().rev().collect::<String>();
    format!("***{suffix}")
}

fn llm_router_provider<'a>(config: &'a MuldexConfig) -> Option<&'a ProviderConfig> {
    config.providers.get("llm-router")
}

fn llm_router_provider_mut<'a>(config: &'a mut MuldexConfig) -> &'a mut ProviderConfig {
    config
        .providers
        .entry("llm-router".to_string())
        .or_insert_with(|| ProviderConfig {
            kind: "openai-compatible".to_string(),
            host: Some("127.0.0.1".to_string()),
            port: Some(3000),
            base_url: None,
            api_key: None,
            api_key_env: None,
            default_model: None,
        })
}

fn interactive_shell_plain_output_enabled() -> bool {
    if matches!(std::env::var("MULDEX_FORCE_TTY_RENDER"), Ok(value) if value == "1") {
        return false;
    }
    if matches!(std::env::var("MULDEX_FORCE_PLAIN_SHELL"), Ok(value) if value == "1") {
        return true;
    }
    !io::stdout().is_terminal()
}

fn interactive_shell_line_input_enabled() -> bool {
    interactive_shell_plain_output_enabled() || !io::stdin().is_terminal()
}

fn interactive_scripted_keys_state() -> Option<InteractiveScriptedKeysState> {
    let raw = std::env::var("MULDEX_SCRIPTED_KEYS").ok()?;
    let mut events = std::collections::VecDeque::new();

    for token in raw.split(',') {
        let token = token.trim();
        if let Some(text) = token.strip_prefix("TEXT:") {
            for ch in text.chars() {
                events.push_back(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
            }
            continue;
        }

        let event = match token {
            "ENTER" => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            "TAB" => KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            "UP" => KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            "DOWN" => KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            "ESC" => KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            "CTRL_R" => KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            "CTRL_U" => KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            "CTRL_C" => KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            _ => continue,
        };
        events.push_back(event);
    }

    Some(InteractiveScriptedKeysState { events })
}

fn interactive_slash_catalog() -> &'static [InteractiveSlashHint] {
    &[
        InteractiveSlashHint { command: "/help", summary: "show available commands" },
        InteractiveSlashHint { command: "/status", summary: "show runtime state" },
        InteractiveSlashHint { command: "/model", summary: "show or set active model" },
        InteractiveSlashHint { command: "/provider", summary: "show, list, or switch provider" },
        InteractiveSlashHint { command: "/approval", summary: "show or set approval mode" },
        InteractiveSlashHint { command: "/compact", summary: "request compaction" },
        InteractiveSlashHint { command: "/sessions", summary: "list resumable sessions" },
        InteractiveSlashHint { command: "/resume", summary: "resume active or named session" },
        InteractiveSlashHint { command: "/new", summary: "create a fresh session" },
        InteractiveSlashHint { command: "/exit", summary: "leave interactive shell" },
    ]
}

fn filtered_interactive_slash_hints(buffer: &InteractivePromptBuffer) -> Vec<InteractiveSlashHint> {
    let first_line = buffer.first_line();
    if !first_line.starts_with('/') {
        return Vec::new();
    }
    interactive_slash_catalog()
        .iter()
        .filter(|hint| hint.command.starts_with(first_line))
        .cloned()
        .collect()
}

fn render_interactive_slash_hint_lines(
    buffer: &InteractivePromptBuffer,
    completion: &InteractiveSlashCompletionState,
) -> Vec<String> {
    if !completion.visible {
        return Vec::new();
    }
    let hints = filtered_interactive_slash_hints(buffer);
    if hints.is_empty() {
        return Vec::new();
    }

    let mut lines = vec!["slash commands:".to_string()];
    for hint in hints {
        let marker = if completion.current_command() == Some(hint.command) {
            ">"
        } else {
            " "
        };
        lines.push(format!("{marker} {} - {}", hint.command, hint.summary));
    }
    lines
}

fn render_interactive_history_search_lines(
    history: &InteractiveHistoryState,
    search: &InteractiveHistorySearchState,
) -> Vec<String> {
    if !search.is_active() {
        return Vec::new();
    }

    let mut lines = vec![format!("reverse search active: {}", search.query)];
    lines.push(format!("matches: {}", search.matches.len()));
    if !search.matches.is_empty() {
        lines.push(format!("match_index: {}/{}", search.match_index + 1, search.matches.len()));
    }
    match search.current_match(history) {
        Some(entry) => lines.push(format!("match: {}", entry)),
        None => lines.push("match: none".to_string()),
    }
    lines.push("search controls: Ctrl+R next, type to refine, Backspace to widen, Esc to restore draft".to_string());
    lines
}

fn apply_interactive_slash_completion(
    buffer: &mut InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
) -> bool {
    if !buffer.first_line().starts_with('/') {
        completion.reset();
        return false;
    }

    if completion.seed == buffer.first_line() && completion.current_command().is_some() {
        buffer.replace_first_line(completion.current_command().expect("current command present"));
        return true;
    }

    if completion.matches.is_empty() || !completion.seed.starts_with('/') {
        completion.update_from_buffer(buffer);
    } else if completion.current_command().is_some_and(|command| command == buffer.first_line()) {
        if completion.matches.len() > 1 {
            completion.select_next();
        }
    } else {
        completion.update_from_buffer(buffer);
    }

    let Some(replacement) = completion.current_command() else {
        completion.reset();
        return false;
    };

    buffer.replace_first_line(replacement);
    true
}

fn move_interactive_slash_selection(
    buffer: &InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
    direction: i32,
) -> bool {
    if !buffer.first_line().starts_with('/') {
        completion.reset();
        return false;
    }
    if completion.matches.is_empty() || !completion.seed.starts_with('/') {
        completion.update_from_buffer(buffer);
    }
    if completion.matches.is_empty() {
        return false;
    }
    if direction < 0 {
        completion.select_previous()
    } else {
        completion.select_next()
    }
}

fn render_interactive_shell_input_frame(
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    buffer: &InteractivePromptBuffer,
    completion: &InteractiveSlashCompletionState,
    history: &InteractiveHistoryState,
    search: &InteractiveHistorySearchState,
) -> Result<(), Box<dyn std::error::Error>> {
    render_interactive_shell_view(driver, shell, buffer, completion, history, search)?;
    render_interactive_prompt_buffer(buffer)?;
    Ok(())
}

fn render_interactive_prompt_buffer(buffer: &InteractivePromptBuffer) -> Result<(), Box<dyn std::error::Error>> {
    if interactive_shell_plain_output_enabled() {
        return Ok(());
    }
    let lines = buffer.text.split('\n').collect::<Vec<_>>();
    let cursor_prefix = &buffer.text[..buffer.cursor];
    let cursor_line = cursor_prefix.chars().filter(|ch| *ch == '\n').count();
    let cursor_column = cursor_prefix
        .rsplit('\n')
        .next()
        .unwrap_or("")
        .chars()
        .count()
        .saturating_add(3);

    print!("\r\x1b[2K");
    for (index, line) in lines.iter().enumerate() {
        if index == 0 {
            print!("> {line}");
        } else {
            print!("\n  {line}");
        }
    }
    print!("\x1b[{}A\r\x1b[{}C", lines.len().saturating_sub(cursor_line + 1), cursor_column);
    io::stdout().flush()?;
    Ok(())
}

fn handle_interactive_key_event(
    key_event: KeyEvent,
    buffer: &mut InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
    history: &mut InteractiveHistoryState,
    search: &mut InteractiveHistorySearchState,
) -> InteractiveKeyAction {
    match key_event {
        KeyEvent {
            code: KeyCode::Char('c' | 'd'),
            modifiers,
            ..
        } if modifiers.contains(KeyModifiers::CONTROL) => InteractiveKeyAction::Exit,
        KeyEvent {
            code: KeyCode::Char('l'),
            modifiers,
            ..
        } if modifiers.contains(KeyModifiers::CONTROL) => InteractiveKeyAction::Status,
        KeyEvent {
            code: KeyCode::Char('r'),
            modifiers,
            ..
        } if modifiers.contains(KeyModifiers::CONTROL) => {
            completion.reset();
            if search.reverse_search(history, buffer) {
                InteractiveKeyAction::RedrawFrame
            } else {
                InteractiveKeyAction::Noop
            }
        }
        KeyEvent { code: KeyCode::Esc, .. } => {
            if search.restore_draft(buffer) {
                completion.reset();
                return InteractiveKeyAction::RedrawFrame;
            }
            if completion.visible && !completion.matches.is_empty() {
                completion.visible = false;
            } else {
                completion.reset();
                search.reset();
                buffer.clear();
            }
            InteractiveKeyAction::RedrawFrame
        }
        KeyEvent { code: KeyCode::Home, .. } => {
            completion.reset();
            search.reset();
            buffer.move_home();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::End, .. } => {
            completion.reset();
            search.reset();
            buffer.move_end();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent {
            code: KeyCode::Char('j'),
            modifiers,
            ..
        } if modifiers.contains(KeyModifiers::CONTROL) => {
            completion.reset();
            search.reset();
            buffer.insert_newline();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent {
            code: KeyCode::Char('u'),
            modifiers,
            ..
        } if modifiers.contains(KeyModifiers::CONTROL) => {
            completion.reset();
            search.reset();
            buffer.clear();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::Backspace, .. } => {
            if search.backspace_query(history, buffer) {
                completion.reset();
                return InteractiveKeyAction::RedrawFrame;
            }
            completion.reset();
            search.reset();
            buffer.backspace();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::Left, modifiers, .. } if modifiers.contains(KeyModifiers::ALT) => {
            completion.reset();
            search.reset();
            buffer.move_word_left();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::Left, .. } => {
            completion.reset();
            search.reset();
            buffer.move_left();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::Up, .. } => {
            if move_interactive_slash_selection(buffer, completion, -1) || history.previous(buffer) {
                if completion.visible {
                    InteractiveKeyAction::RedrawFrame
                } else {
                    InteractiveKeyAction::RedrawPrompt
                }
            } else {
                InteractiveKeyAction::Noop
            }
        }
        KeyEvent { code: KeyCode::Right, modifiers, .. } if modifiers.contains(KeyModifiers::ALT) => {
            completion.reset();
            search.reset();
            buffer.move_word_right();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::Right, .. } => {
            completion.reset();
            search.reset();
            buffer.move_right();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::Down, .. } => {
            if move_interactive_slash_selection(buffer, completion, 1) || history.next(buffer) {
                if completion.visible {
                    InteractiveKeyAction::RedrawFrame
                } else {
                    InteractiveKeyAction::RedrawPrompt
                }
            } else {
                InteractiveKeyAction::Noop
            }
        }
        KeyEvent { code: KeyCode::Char('w'), modifiers, .. } if modifiers.contains(KeyModifiers::CONTROL) => {
            completion.reset();
            history.index_from_end = None;
            history.draft = None;
            search.reset();
            buffer.delete_word_left();
            InteractiveKeyAction::RedrawPrompt
        }
        KeyEvent { code: KeyCode::Tab, .. } => {
            if apply_interactive_slash_completion(buffer, completion) {
                InteractiveKeyAction::RedrawFrame
            } else {
                InteractiveKeyAction::Noop
            }
        }
        KeyEvent { code: KeyCode::Enter, .. } => {
            if buffer.first_line().starts_with('/')
                && completion.current_command().is_some()
                && completion.current_command() != Some(buffer.first_line())
            {
                buffer.replace_first_line(completion.current_command().expect("current command present"));
                return InteractiveKeyAction::RedrawFrame;
            }
            let input = parse_interactive_shell_input(&buffer.text);
            completion.reset();
            search.reset();
            buffer.clear();
            history.index_from_end = None;
            history.draft = None;
            InteractiveKeyAction::Submit(input)
        }
        KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
            ..
        } if !modifiers.contains(KeyModifiers::CONTROL) => {
            if search.extend_query(history, buffer, ch) {
                completion.reset();
                return InteractiveKeyAction::RedrawFrame;
            }
            completion.reset();
            history.index_from_end = None;
            history.draft = None;
            search.reset();
            buffer.insert_char(ch);
            if buffer.first_line().starts_with('/') {
                completion.update_from_buffer(buffer);
                InteractiveKeyAction::RedrawFrame
            } else {
                InteractiveKeyAction::RedrawPrompt
            }
        }
        _ => InteractiveKeyAction::Noop,
    }
}

fn read_interactive_shell_input_event(
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    buffer: &mut InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
    history: &mut InteractiveHistoryState,
    search: &mut InteractiveHistorySearchState,
) -> Result<Option<InteractiveShellInput>, Box<dyn std::error::Error>> {
    if !event::poll(Duration::from_millis(250))? {
        return Ok(None);
    }
    loop {
        match event::read()? {
            Event::Key(key_event) => match handle_interactive_key_event(key_event, buffer, completion, history, search) {
                InteractiveKeyAction::Noop => {}
                InteractiveKeyAction::RedrawPrompt => {
                    render_interactive_prompt_buffer(buffer)?;
                }
                InteractiveKeyAction::RedrawFrame => {
                    render_interactive_shell_input_frame(driver, shell, buffer, completion, history, search)?;
                }
                InteractiveKeyAction::Exit => {
                    println!();
                    return Ok(Some(InteractiveShellInput::Exit));
                }
                InteractiveKeyAction::Status => {
                    print!("\x1b[2J\x1b[H");
                    io::stdout().flush()?;
                    return Ok(Some(InteractiveShellInput::Status));
                }
                InteractiveKeyAction::Submit(input) => {
                    println!();
                    return Ok(Some(input));
                }
            },
            _ => {}
        }
    }
}

fn read_interactive_shell_scripted_event(
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    buffer: &mut InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
    history: &mut InteractiveHistoryState,
    search: &mut InteractiveHistorySearchState,
    scripted: &mut InteractiveScriptedKeysState,
) -> Result<Option<InteractiveShellInput>, Box<dyn std::error::Error>> {
    let Some(key_event) = scripted.events.pop_front() else {
        return Ok(Some(InteractiveShellInput::Exit));
    };

    match handle_interactive_key_event(key_event, buffer, completion, history, search) {
        InteractiveKeyAction::Noop => Ok(None),
        InteractiveKeyAction::RedrawPrompt => {
            render_interactive_prompt_buffer(buffer)?;
            Ok(None)
        }
        InteractiveKeyAction::RedrawFrame => {
            render_interactive_shell_input_frame(driver, shell, buffer, completion, history, search)?;
            Ok(None)
        }
        InteractiveKeyAction::Exit => Ok(Some(InteractiveShellInput::Exit)),
        InteractiveKeyAction::Status => Ok(Some(InteractiveShellInput::Status)),
        InteractiveKeyAction::Submit(input) => Ok(Some(input)),
    }
}

fn load_interactive_shell_store() -> Result<Option<InteractiveShellStore>, Box<dyn std::error::Error>> {
    let path = interactive_shell_snapshot_path();
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)?;
    let store: InteractiveShellStore = serde_json::from_str(&raw)?;
    Ok(Some(store))
}

fn save_interactive_shell_store(
    store: &InteractiveShellStore,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        interactive_shell_snapshot_path(),
        serde_json::to_string_pretty(store)?,
    )?;
    Ok(())
}

fn interactive_shell_store_with_default_session() -> InteractiveShellStore {
    let snapshot = InteractiveShellSnapshot {
        schema_version: "muldex-interactive-shell-v1".to_string(),
        session_id: interactive_shell_session_id(),
        shell: interactive_shell_state(),
        runtime: interactive_shell_driver().state,
        messages: vec![InteractiveMessage {
            role: InteractiveMessageRole::System,
            content: "interactive shell created".to_string(),
        }],
        prompt_history: Vec::new(),
    };
    InteractiveShellStore {
        schema_version: "muldex-interactive-shell-store-v1".to_string(),
        active_session_id: snapshot.session_id.clone(),
        sessions: vec![snapshot],
    }
}

fn active_interactive_shell_snapshot(
    store: &InteractiveShellStore,
) -> Result<InteractiveShellSnapshot, Box<dyn std::error::Error>> {
    store
        .sessions
        .iter()
        .find(|snapshot| snapshot.session_id == store.active_session_id)
        .cloned()
        .ok_or_else(|| format!("interactive shell active session not found: {}", store.active_session_id).into())
}

fn save_active_interactive_shell_snapshot(
    shell: &InteractiveShellState,
    runtime: &RuntimeState,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let active_session_id = store.active_session_id.clone();
    if let Some(snapshot) = store
        .sessions
        .iter_mut()
        .find(|snapshot| snapshot.session_id == active_session_id)
    {
        snapshot.shell = shell.clone();
        snapshot.runtime = runtime.clone();
    }
    save_interactive_shell_store(&store)?;
    Ok(())
}

fn append_message_to_active_session(
    role: InteractiveMessageRole,
    content: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let active_session_id = store.active_session_id.clone();
    if let Some(snapshot) = store
        .sessions
        .iter_mut()
        .find(|snapshot| snapshot.session_id == active_session_id)
    {
        snapshot.messages.push(InteractiveMessage { role, content });
    }
    save_interactive_shell_store(&store)?;
    Ok(())
}

fn append_prompt_history_to_active_session(entry: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let active_session_id = store.active_session_id.clone();
    if let Some(snapshot) = store
        .sessions
        .iter_mut()
        .find(|snapshot| snapshot.session_id == active_session_id)
    {
        let normalized = entry.trim();
        if !normalized.is_empty()
            && snapshot
                .prompt_history
                .last()
                .is_none_or(|last| last != normalized)
        {
            snapshot.prompt_history.push(normalized.to_string());
        }
    }
    save_interactive_shell_store(&store)?;
    Ok(())
}

fn print_interactive_shell_sessions() -> Result<(), Box<dyn std::error::Error>> {
    match load_interactive_shell_store()? {
        Some(store) => {
            for snapshot in store.sessions {
                if interactive_shell_plain_output_enabled() {
                    println!("session.id: {}", snapshot.session_id);
                    println!("session.phase: {:?}", snapshot.runtime.phase);
                    println!("session.cycle_index: {}", snapshot.runtime.cycle_index);
                    println!(
                        "session.model: {}",
                        runtime_model_label(&snapshot.runtime, &snapshot.shell)
                    );
                    println!("session.active: {}", snapshot.session_id == store.active_session_id);
                    println!("session.message_count: {}", snapshot.messages.len());
                }
            }
        }
        None => {
            if interactive_shell_plain_output_enabled() {
                println!("session.none: true");
            }
        }
    }
    Ok(())
}

fn print_interactive_shell_banner() {
    if interactive_shell_plain_output_enabled() {
        println!("muldex interactive shell");
        println!("type /help for commands, /exit to leave");
    }
}

fn print_interactive_shell_header(driver: &RuntimeDriver, shell: &InteractiveShellState, session_id: &str) {
    println!("== muldex session ==");
    println!("session.id: {}", session_id);
    println!("session.phase: {:?}", driver.state.phase);
    println!("session.model: {}", runtime_model_label(&driver.state, shell));
    println!(
        "session.approval_mode: {}",
        approval_policy_label(&driver.state.request.safety.approval_policy)
    );
    println!("---------------------");
}

fn print_interactive_message_log(messages: &[InteractiveMessage]) {
    let recent = messages.iter().rev().take(6).collect::<Vec<_>>();
    for message in recent.into_iter().rev() {
        let role = match message.role {
            InteractiveMessageRole::System => "system",
            InteractiveMessageRole::User => "user",
            InteractiveMessageRole::Assistant => "assistant",
        };
        println!("[{role}] {}", message.content);
    }
}

fn render_interactive_shell_view(
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    buffer: &InteractivePromptBuffer,
    completion: &InteractiveSlashCompletionState,
    history: &InteractiveHistoryState,
    search: &InteractiveHistorySearchState,
) -> Result<(), Box<dyn std::error::Error>> {
    if interactive_shell_plain_output_enabled() {
        return Ok(());
    }

    let store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let snapshot = active_interactive_shell_snapshot(&store)?;
    print!("\x1b[2J\x1b[H");
    print_interactive_shell_header(driver, shell, &snapshot.session_id);
    print_interactive_message_log(&snapshot.messages);
    println!();
    println!("commands: /help /status /model /approval /compact /sessions /resume /new /exit");
    for line in render_interactive_slash_hint_lines(buffer, completion) {
        println!("{line}");
    }
    for line in render_interactive_history_search_lines(history, search) {
        println!("{line}");
    }
    Ok(())
}

fn print_interactive_shell_help() {
    if interactive_shell_plain_output_enabled() {
        println!("/help     show available commands");
        println!("/status   show runtime phase and cycle");
        println!("/config llm show current llm-router config");
        println!("/provider show current provider or switch with /provider use <name>");
        println!("/model    show or set active model");
        println!("/approval show or set approval mode");
        println!("/compact  record a compaction request");
        println!("/sessions list resumable interactive shells");
        println!("/resume   restore active shell or a named session");
        println!("/new      create a fresh interactive shell session");
        println!("/exit     leave interactive shell");
    }
}

fn print_interactive_shell_status(driver: &RuntimeDriver, shell: &InteractiveShellState) {
    let config = load_muldex_config().unwrap_or_default();
    if interactive_shell_plain_output_enabled() {
        println!("session.phase: {:?}", driver.state.phase);
        println!("session.cycle_index: {}", driver.state.cycle_index);
        println!("session.objective: {}", driver.state.request.objective);
        println!("session.model: {}", runtime_model_label(&driver.state, shell));
        println!(
            "session.approval_mode: {}",
            approval_policy_label(&driver.state.request.safety.approval_policy)
        );
        println!(
            "session.requires_explicit_approval_for_next_step: {}",
            driver
                .state
                .request
                .safety
                .requires_explicit_approval_for_next_step
        );
        println!("session.compact_count: {}", shell.compact_count);
        println!("session.resume_count: {}", shell.resume_count);
        println!(
            "session.pending_post_compaction: {}",
            driver.state.request.post_compaction.pending_post_compaction
        );
        println!(
            "session.first_post_compaction_turn: {}",
            driver.state.request.post_compaction.first_post_compaction_turn
        );
        println!(
            "session.compaction_window_id: {:?}",
            driver.state.request.post_compaction.compaction_window_id
        );
        match llm_router_provider(&config) {
            Some(router) => {
                println!("llm_router.host: {:?}", router.host);
                println!("llm_router.port: {:?}", router.port);
                println!("llm_router.api_key: {}", masked_api_key(router.api_key.as_deref().unwrap_or("")));
                println!("llm_router.default_model: {:?}", router.default_model);
            }
            None => {
                println!("llm_router.configured: false");
            }
        }
        println!("default_provider: {:?}", config.default_provider);
        if let Some(active_name) = active_provider_name(&config) {
            if let Some(provider) = config.providers.get(active_name) {
                println!("active_provider.kind: {}", provider.kind);
                println!("active_provider.base_url: {:?}", provider.base_url);
                println!("active_provider.host: {:?}", provider.host);
                println!("active_provider.port: {:?}", provider.port);
                println!("active_provider.default_model: {:?}", provider.default_model);
            }
        }
        if let Some(report) = driver.state.latest_report.as_ref() {
            println!("session.last_outcome: {:?}", report.outcome);
            println!("session.last_rationale: {}", report.rationale);
        }
    }
}

fn handle_interactive_llm_config_command(
    command: InteractiveLlmConfigCommand,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut config = load_muldex_config()?;
    let message = match command {
        InteractiveLlmConfigCommand::Show => {
            let router = llm_router_provider_mut(&mut config);
            if router.host.as_deref().unwrap_or("").is_empty() {
                "llm-router config not set".to_string()
            } else {
                format!(
                    "llm-router host={} port={} api_key={} default_model={:?}",
                    router.host.as_deref().unwrap_or(""),
                    router.port.unwrap_or_default(),
                    masked_api_key(router.api_key.as_deref().unwrap_or("")),
                    router.default_model
                )
            }
        }
        InteractiveLlmConfigCommand::Test => {
            let Some(router) = llm_router_provider(&config) else {
                return Ok("llm-router test failed: router config missing".to_string());
            };
            let Some(host) = router.host.as_deref() else {
                return Ok("llm-router test failed: host missing".to_string());
            };
            let Some(port) = router.port else {
                return Ok("llm-router test failed: port missing".to_string());
            };

            let mut addrs = format!("{host}:{port}").to_socket_addrs()?;
            let Some(addr) = addrs.next() else {
                return Ok("llm-router test failed: no socket address resolved".to_string());
            };

            match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
                Ok(_) => format!("llm-router test ok: {host}:{port} reachable"),
                Err(error) => format!("llm-router test failed: {host}:{port} unreachable ({error})"),
            }
        }
        InteractiveLlmConfigCommand::SetHost(host) => {
            let router = llm_router_provider_mut(&mut config);
            router.host = Some(host.clone());
            save_muldex_config(&config)?;
            format!("llm-router host set to {host}")
        }
        InteractiveLlmConfigCommand::SetPort(port) => {
            let router = llm_router_provider_mut(&mut config);
            router.port = Some(port);
            save_muldex_config(&config)?;
            format!("llm-router port set to {port}")
        }
        InteractiveLlmConfigCommand::SetApiKey(api_key) => {
            let router = llm_router_provider_mut(&mut config);
            router.api_key = Some(api_key);
            let masked = masked_api_key(router.api_key.as_deref().unwrap_or(""));
            save_muldex_config(&config)?;
            format!("llm-router api-key set to {masked}")
        }
        InteractiveLlmConfigCommand::SetDefaultModel(model) => {
            let router = llm_router_provider_mut(&mut config);
            router.default_model = Some(model.clone());
            save_muldex_config(&config)?;
            format!("llm-router default-model set to {model}")
        }
        InteractiveLlmConfigCommand::Invalid(reason) => format!("llm-router config error: {reason}"),
    };
    Ok(message)
}

fn active_provider_name(config: &MuldexConfig) -> Option<&str> {
    config.default_provider.as_deref()
}

fn provider_socket_address(provider: &ProviderConfig) -> Result<String, Box<dyn std::error::Error>> {
    if let (Some(host), Some(port)) = (provider.host.as_deref(), provider.port) {
        return Ok(format!("{host}:{port}"));
    }

    if let Some(base_url) = provider.base_url.as_deref() {
        let trimmed = base_url
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let authority = trimmed.split('/').next().unwrap_or("");
        if authority.is_empty() {
            return Err("provider base_url missing authority".into());
        }
        if authority.contains(':') {
            return Ok(authority.to_string());
        }
        if base_url.starts_with("https://") {
            return Ok(format!("{authority}:443"));
        }
        return Ok(format!("{authority}:80"));
    }

    Err("provider has no host/port or base_url".into())
}

fn test_provider_connectivity(name: &str, provider: &ProviderConfig) -> String {
    let address = match provider_socket_address(provider) {
        Ok(address) => address,
        Err(error) => return format!("provider test failed: {name} missing endpoint ({error})"),
    };

    match address.to_socket_addrs() {
        Ok(mut addrs) => {
            let Some(addr) = addrs.next() else {
                return format!("provider test failed: {name} no socket address resolved");
            };
            match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
                Ok(_) => format!("provider test ok: {name} reachable at {address}"),
                Err(error) => format!("provider test failed: {name} unreachable at {address} ({error})"),
            }
        }
        Err(error) => format!("provider test failed: {name} address resolution failed ({error})"),
    }
}

fn handle_interactive_provider_command(
    command: InteractiveProviderCommand,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut config = load_muldex_config()?;
    let message = match command {
        InteractiveProviderCommand::Show => {
            let provider_name = active_provider_name(&config).unwrap_or("not-set");
            format!("default provider: {provider_name}")
        }
        InteractiveProviderCommand::List => {
            if config.providers.is_empty() {
                "providers: none".to_string()
            } else {
                let mut names = config.providers.keys().cloned().collect::<Vec<_>>();
                names.sort();
                format!("providers: {}", names.join(", "))
            }
        }
        InteractiveProviderCommand::Use(name) => {
            if !config.providers.contains_key(&name) {
                format!("provider switch failed: {name} not found")
            } else {
                config.default_provider = Some(name.clone());
                save_muldex_config(&config)?;
                format!("default provider set to {name}")
            }
        }
        InteractiveProviderCommand::Test(name) => {
            let target_name = name
                .or_else(|| config.default_provider.clone())
                .unwrap_or_else(|| "llm-router".to_string());
            match config.providers.get(&target_name) {
                Some(provider) => test_provider_connectivity(&target_name, provider),
                None => format!("provider test failed: {target_name} not found"),
            }
        }
        InteractiveProviderCommand::Invalid(reason) => {
            format!("provider command error: {reason}")
        }
    };
    Ok(message)
}

fn handle_interactive_slash_command(
    driver: &mut RuntimeDriver,
    shell: &mut InteractiveShellState,
    command: InteractiveSlashCommand,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let mut system_messages = Vec::<String>::new();
    match command {
        InteractiveSlashCommand::Model(None) => {
            println!("session.model: {}", runtime_model_label(&driver.state, shell));
            system_messages.push(format!("/model -> {}", runtime_model_label(&driver.state, shell)));
        }
        InteractiveSlashCommand::Model(Some(model)) => {
            shell.model = model.clone();
            ensure_runtime_model_state(&mut driver.state, shell);
            if let Some(continuation) = driver.state.request.codex_continuation.as_mut() {
                continuation.source_model = model.clone();
            }
            println!("session.model_set: {}", model);
            system_messages.push(format!("/model set to {model}"));
        }
        InteractiveSlashCommand::Approval(None) => {
            println!(
                "session.approval_mode: {}",
                approval_policy_label(&driver.state.request.safety.approval_policy)
            );
            system_messages.push(format!(
                "/approval -> {}",
                approval_policy_label(&driver.state.request.safety.approval_policy)
            ));
        }
        InteractiveSlashCommand::Approval(Some(mode)) => {
            match parse_approval_policy(&mode) {
                Some(policy) => {
                    shell.approval_mode = approval_policy_label(&policy).to_string();
                    driver.state.request.safety.approval_policy = policy.clone();
                    driver.state.request.safety.requires_explicit_approval_for_next_step =
                        matches!(policy, ApprovalPolicyDescriptor::Ask);
                    println!("session.approval_mode_set: {}", shell.approval_mode);
                    system_messages.push(format!("/approval set to {}", shell.approval_mode));
                }
                None => {
                    println!("session.approval_mode_set: invalid");
                    println!("session.approval_mode_error: unsupported_mode");
                    system_messages.push(format!("/approval invalid mode: {mode}"));
                }
            }
        }
        InteractiveSlashCommand::Compact => {
            shell.compact_count = shell.compact_count.saturating_add(1);
            driver.state.request.post_compaction.pending_post_compaction = true;
            driver.state.request.post_compaction.first_post_compaction_turn = true;
            driver.state.request.post_compaction.compaction_window_id =
                Some(format!("shell-window-{}", shell.compact_count));
            println!("session.compaction_requested: true");
            println!("session.compact_count: {}", shell.compact_count);
            println!(
                "session.compaction_window_id: {:?}",
                driver.state.request.post_compaction.compaction_window_id
            );
            system_messages.push(format!(
                "/compact requested {:?}",
                driver.state.request.post_compaction.compaction_window_id
            ));
        }
        InteractiveSlashCommand::Sessions => {
            print_interactive_shell_sessions()?;
            if !interactive_shell_plain_output_enabled() {
                for snapshot in &store.sessions {
                    system_messages.push(format!(
                        "/sessions {} phase={:?} model={} active={}",
                        snapshot.session_id,
                        snapshot.runtime.phase,
                        runtime_model_label(&snapshot.runtime, &snapshot.shell),
                        snapshot.session_id == store.active_session_id,
                    ));
                }
            } else {
                system_messages.push("/sessions listed available sessions".to_string());
            }
        }
        InteractiveSlashCommand::Resume(session_id) => {
            let target_session_id = session_id.unwrap_or_else(|| store.active_session_id.clone());
            match store
                .sessions
                .iter()
                .find(|snapshot| snapshot.session_id == target_session_id)
                .cloned()
            {
                Some(snapshot) => {
                    store.active_session_id = snapshot.session_id.clone();
                    *shell = snapshot.shell;
                    driver.state = snapshot.runtime;
                    shell.resume_count = shell.resume_count.saturating_add(1);
                    println!("session.resume_requested: true");
                    println!("session.resumed: true");
                    println!("session.id: {}", store.active_session_id);
                    println!("session.resume_count: {}", shell.resume_count);
                    system_messages.push(format!("/resume -> {}", store.active_session_id));
                }
                None => {
                    println!("session.resume_requested: true");
                    println!("session.resumed: false");
                    println!("session.resume_reason: session_not_found");
                    system_messages.push(format!("/resume failed for {}", target_session_id));
                }
            }
        }
        InteractiveSlashCommand::New => {
            let snapshot = InteractiveShellSnapshot {
                schema_version: "muldex-interactive-shell-v1".to_string(),
                session_id: interactive_shell_session_id(),
                shell: interactive_shell_state(),
                runtime: interactive_shell_driver().state,
                messages: vec![InteractiveMessage {
                    role: InteractiveMessageRole::System,
                    content: "interactive shell created".to_string(),
                }],
                prompt_history: Vec::new(),
            };
            store.active_session_id = snapshot.session_id.clone();
            *shell = snapshot.shell.clone();
            driver.state = snapshot.runtime.clone();
            println!("session.new: true");
            println!("session.id: {}", snapshot.session_id);
            store.sessions.push(snapshot);
        }
        InteractiveSlashCommand::ConfigLlm(command) => {
            let message = handle_interactive_llm_config_command(command)?;
            println!("{message}");
            system_messages.push(message);
        }
        InteractiveSlashCommand::Provider(command) => {
            let message = handle_interactive_provider_command(command)?;
            println!("{message}");
            system_messages.push(message);
        }
        InteractiveSlashCommand::Unknown(command) => {
            println!("slash command not implemented yet: {command}");
        }
    }

    if let Some(snapshot) = store
        .sessions
        .iter_mut()
        .find(|snapshot| snapshot.session_id == store.active_session_id)
    {
        snapshot.shell = shell.clone();
        snapshot.runtime = driver.state.clone();
        for content in system_messages {
            snapshot.messages.push(InteractiveMessage {
                role: InteractiveMessageRole::System,
                content,
            });
        }
    }
    save_interactive_shell_store(&store)?;
    Ok(())
}

fn handle_interactive_prompt(
    driver: &mut RuntimeDriver,
    shell: &InteractiveShellState,
    prompt: String,
) -> Result<(), Box<dyn std::error::Error>> {
    driver.state.request.objective = prompt.clone();
    driver.state.request.continue_reason = ContinueReason::ManualUserRequest;
    append_message_to_active_session(InteractiveMessageRole::User, prompt.clone())?;
    append_prompt_history_to_active_session(prompt.clone())?;
    let result = driver.advance(ContinueDecision {
        allow_continue: true,
        mode: ContinueMode::NextTurn,
        rationale: format!("interactive prompt: {prompt}"),
        next_action: Some("continue interactive session".to_string()),
        pause_for_approval: false,
        consume_interrupts_now: false,
        may_continue_other_work: true,
        suppress_duplicate_injection: false,
        downgrade_trigger_turn: false,
        request_compaction: false,
        request_handoff_summary: false,
        request_checkpoint: false,
        enter_self_correction: false,
    });
    println!("assistant.phase: {:?}", result.updated_state.phase);
    println!("assistant.cycle_index: {}", result.updated_state.cycle_index);
    println!("assistant.outcome: {:?}", result.report.outcome);
    println!("assistant.summary: {}", result.report.rationale);
    append_message_to_active_session(
        InteractiveMessageRole::Assistant,
        result.report.rationale.clone(),
    )?;
    save_active_interactive_shell_snapshot(shell, &driver.state)?;
    Ok(())
}

fn run_interactive_shell(initial_prompt: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let snapshot = active_interactive_shell_snapshot(&store)?;
    let session_id = snapshot.session_id.clone();
    let messages = snapshot.messages.clone();
    let prompt_history = snapshot.prompt_history.clone();
    let mut driver = RuntimeDriver::new(snapshot.runtime);
    let mut shell = snapshot.shell;
    let mut prompt_buffer = InteractivePromptBuffer::default();
    let mut completion_state = InteractiveSlashCompletionState::default();
    let mut history_state = InteractiveHistoryState::from_entries(prompt_history);
    let mut history_search_state = InteractiveHistorySearchState::default();
    let mut scripted_keys_state = interactive_scripted_keys_state();
    print_interactive_shell_banner();
    print_interactive_shell_header(&driver, &shell, &session_id);
    print_interactive_message_log(&messages);
    if llm_router_provider(&load_muldex_config()?).is_none() {
        println!("llm-router not configured; use /config llm host <ip>, /config llm port <port>, /config llm api-key <key>");
    }
    render_interactive_shell_view(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
    save_active_interactive_shell_snapshot(&shell, &driver.state)?;

    if let Some(prompt) = initial_prompt {
        handle_interactive_prompt(&mut driver, &shell, prompt)?;
        render_interactive_shell_view(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
    }

    let stdin = io::stdin();
    let use_line_input = scripted_keys_state.is_none() && interactive_shell_line_input_enabled();
    let _raw_mode_guard = if scripted_keys_state.is_none() && !use_line_input {
        let guard = RawModeGuard::activate()?;
        render_interactive_shell_input_frame(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
        Some(guard)
    } else {
        None
    };

    loop {
        let input = if let Some(scripted) = scripted_keys_state.as_mut() {
            read_interactive_shell_scripted_event(
                &driver,
                &shell,
                &mut prompt_buffer,
                &mut completion_state,
                &mut history_state,
                &mut history_search_state,
                scripted,
            )?
        } else if use_line_input {
            print!("> ");
            io::stdout().flush()?;

            let mut line = String::new();
            let bytes = stdin.read_line(&mut line)?;
            if bytes == 0 {
                println!("leaving muldex interactive shell");
                println!("To continue this session, run muldex resume {session_id}");
                break;
            }
            Some(parse_interactive_shell_input(&line))
        } else {
            read_interactive_shell_input_event(
                &driver,
                &shell,
                &mut prompt_buffer,
                &mut completion_state,
                &mut history_state,
                &mut history_search_state,
            )?
        };

        let Some(input) = input else {
            continue;
        };

        match input {
            InteractiveShellInput::Empty => {}
            InteractiveShellInput::Exit => {
                println!("leaving muldex interactive shell");
                println!("To continue this session, run muldex resume {session_id}");
                break;
            }
            InteractiveShellInput::Help => {
                print_interactive_shell_help();
                render_interactive_shell_view(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
            }
            InteractiveShellInput::Status => {
                print_interactive_shell_status(&driver, &shell);
                render_interactive_shell_view(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
            }
            InteractiveShellInput::SlashCommand(command) => {
                handle_interactive_slash_command(&mut driver, &mut shell, command)?;
                render_interactive_shell_view(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
            }
            InteractiveShellInput::Prompt(prompt) => {
                history_state.record_submission(&prompt);
                handle_interactive_prompt(&mut driver, &shell, prompt)?;
                render_interactive_shell_view(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
            }
        }

        if !use_line_input {
            render_interactive_shell_input_frame(&driver, &shell, &prompt_buffer, &completion_state, &history_state, &history_search_state)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use muldex_core::protocol::ApprovalPolicyDescriptor;
    use muldex_core::protocol::ContextPressure;
    use muldex_core::protocol::ContinueReason;
    use muldex_core::protocol::ContinueRequest;
    use muldex_core::protocol::ExecutionMode;
    use muldex_core::protocol::InterruptQueueState;
    use muldex_core::protocol::PendingApprovalState;
    use muldex_core::protocol::PermissionContextSnapshot;
    use muldex_core::protocol::PostCompactionState;
    use muldex_core::protocol::ProgressSnapshot;
    use muldex_core::protocol::RecoverySnapshot;
    use muldex_core::protocol::RuntimeModeState;
    use muldex_core::protocol::SandboxModeDescriptor;
    use muldex_core::protocol::SelfCorrectionState;
    use muldex_core::protocol::StateChangeKind;
    use muldex_runtime::client_views::ClientResponsePayloadView;
    use muldex_runtime::client_views::response_view;
    use muldex_runtime::daemon::RuntimeDaemon;
    use muldex_runtime::continuity::ExportedReportView;
    use muldex_runtime::runtime::RuntimePhase;
    use muldex_runtime::runtime::RuntimeState;
    use std::sync::{Mutex, OnceLock};

    fn config_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn sample_state() -> RuntimeState {
        RuntimeState {
            request: ContinueRequest {
                thread_id: "thread-1".to_string(),
                turn_id: "turn-1".to_string(),
                objective: "continue task".to_string(),
                constraints: vec!["do not spin".to_string()],
                continue_reason: ContinueReason::PendingInput,
                recent_state_changes: vec![StateChangeKind::NewConfirmedFinding],
                working_hypothesis: None,
                last_agent_message: None,
                pending_input_count: 0,
                trigger_turn_pending: false,
                tool_call_count_this_turn: 0,
                context_pressure: ContextPressure::default(),
                duplicate_injection_detected: false,
                repeated_follow_up_count: 0,
                progress: ProgressSnapshot {
                    completed_steps: 0,
                    total_steps_hint: None,
                    last_meaningful_progress_at_ms: None,
                    no_progress_iteration_count: 0,
                },
                recovery: RecoverySnapshot {
                    last_recovery_reason: None,
                    recovery_attempt_count: 0,
                    last_recovery_had_progress: true,
                },
                last_checkpoint: None,
                self_correction: SelfCorrectionState {
                    active: false,
                    correction_attempt_count: 0,
                    last_correction_target: None,
                    last_correction_had_progress: false,
                },
                post_compaction: PostCompactionState::default(),
                runtime_mode: RuntimeModeState {
                    active_agent_mode: Some("build".to_string()),
                    previous_agent_mode: None,
                    active_execution_mode: Some(ExecutionMode::Interactive),
                    previous_execution_mode: None,
                    mode_transition_pending_guidance: false,
                    invoked_skills: Vec::new(),
                },
                pending_approval: PendingApprovalState::default(),
                interrupts: InterruptQueueState::default(),
                last_run_report: None,
                safety: PermissionContextSnapshot {
                    sandbox_mode: SandboxModeDescriptor::WorkspaceWrite,
                    approval_policy: ApprovalPolicyDescriptor::OnRequest,
                    permission_profile_summary: "managed".to_string(),
                    network_access_enabled: false,
                    requires_explicit_approval_for_next_step: false,
                },
                codex_continuation: None,
            },
            cycle_index: 0,
            phase: RuntimePhase::Ready,
            latest_report: None,
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    fn cleanup(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        let transport_root = transport_root_for_snapshot(path);
        let _ = std::fs::remove_dir_all(&transport_root);
        let runtime_root = path
            .parent()
            .map(|parent| {
                let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(".muldex-daemon");
                parent.join(format!("{stem}.muldex-daemon"))
            })
            .unwrap_or_else(|| PathBuf::from(".muldex-daemon"));
        let _ = std::fs::remove_dir_all(runtime_root);
    }

    #[test]
    fn client_send_command_and_read_response_round_trip() {
        let path = temp_root("muldex-cli-client-roundtrip.json");
        cleanup(&path);

        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-1", sample_state())
            .expect("create session");
        daemon.save().expect("save daemon");

        client_send_command(
            path.clone(),
            "cmd-cli-1".to_string(),
            "session-1".to_string(),
            DaemonCommandKindArg::Status,
            ClientAccessModeArg::ReadOnly,
        )
        .expect("send command");

        let transport = FileCommandTransport::new(transport_root_for_snapshot(&path));
        let commands = transport.list_commands().expect("list commands");
        assert_eq!(commands.len(), 1);

        daemon.serve_once().expect("serve once");

        let response = transport.read_response("cmd-cli-1").expect("read response");
        let view = response_view(response);
        assert!(view.ok);
        match view.payload.expect("payload") {
            ClientResponsePayloadView::Step { phase, cycle_index, .. } => {
                assert_eq!(phase, "Running");
                assert_eq!(cycle_index, 1);
            }
            _ => panic!("expected step payload"),
        }

        daemon.shutdown().expect("shutdown daemon");
        cleanup(&path);
    }

    #[test]
    fn client_send_command_rejects_mutation_in_read_only_mode() {
        let path = temp_root("muldex-cli-client-read-only.json");
        cleanup(&path);

        let error = client_send_command(
            path,
            "cmd-cli-2".to_string(),
            "session-1".to_string(),
            DaemonCommandKindArg::AdvanceSample,
            ClientAccessModeArg::ReadOnly,
        )
        .expect_err("read-only gate should reject mutation");

        assert!(error
            .to_string()
            .contains("client access mode ReadOnly does not allow command kind advance-sample"));
    }

    #[test]
    fn interactive_shell_input_parses_core_commands() {
        assert_eq!(parse_interactive_shell_input("   "), InteractiveShellInput::Empty);
        assert_eq!(parse_interactive_shell_input("/help"), InteractiveShellInput::Help);
        assert_eq!(parse_interactive_shell_input("/status"), InteractiveShellInput::Status);
        assert_eq!(parse_interactive_shell_input("/exit"), InteractiveShellInput::Exit);
        assert_eq!(
            parse_interactive_shell_input("/model"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Model(None))
        );
        assert_eq!(
            parse_interactive_shell_input("/model gpt-5"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Model(Some(
                "gpt-5".to_string()
            )))
        );
        assert_eq!(
            parse_interactive_shell_input("/approval on-request"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Approval(Some(
                "on-request".to_string()
            )))
        );
        assert_eq!(
            parse_interactive_shell_input("/compact"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Compact)
        );
        assert_eq!(
            parse_interactive_shell_input("/sessions"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Sessions)
        );
        assert_eq!(
            parse_interactive_shell_input("/resume"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Resume(None))
        );
        assert_eq!(
            parse_interactive_shell_input("/resume session-2"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Resume(Some(
                "session-2".to_string()
            )))
        );
        assert_eq!(
            parse_interactive_shell_input("/new"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::New)
        );
        assert_eq!(
            parse_interactive_shell_input("/config llm host 127.0.0.1"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::ConfigLlm(
                InteractiveLlmConfigCommand::SetHost("127.0.0.1".to_string())
            ))
        );
        assert_eq!(
            parse_interactive_shell_input("/config llm test"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::ConfigLlm(
                InteractiveLlmConfigCommand::Test
            ))
        );
        assert_eq!(
            parse_interactive_shell_input("/provider use openai-prod"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Provider(
                InteractiveProviderCommand::Use("openai-prod".to_string())
            ))
        );
        assert_eq!(
            parse_interactive_shell_input("/provider test openai-prod"),
            InteractiveShellInput::SlashCommand(InteractiveSlashCommand::Provider(
                InteractiveProviderCommand::Test(Some("openai-prod".to_string()))
            ))
        );
        assert_eq!(
            parse_interactive_shell_input("hello runtime"),
            InteractiveShellInput::Prompt("hello runtime".to_string())
        );
        assert_eq!(
            parse_interactive_shell_input("/model\nsecond line"),
            InteractiveShellInput::Prompt("/model\nsecond line".trim().to_string())
        );
    }

    #[test]
    fn slash_command_messages_persist_through_store_save() {
        let path = temp_root("muldex-cli-interactive-store.json");
        cleanup(&path);
        unsafe {
            std::env::set_var("MULDEX_INTERACTIVE_SHELL_PATH", &path);
        }

        let store = interactive_shell_store_with_default_session();
        save_interactive_shell_store(&store).expect("save store");
        let snapshot = active_interactive_shell_snapshot(&store).expect("active snapshot");
        let mut driver = RuntimeDriver::new(snapshot.runtime.clone());
        let mut shell = snapshot.shell.clone();

        handle_interactive_slash_command(
            &mut driver,
            &mut shell,
            InteractiveSlashCommand::Model(Some("gpt-5-persist".to_string())),
        )
        .expect("handle slash command");

        let loaded_store = load_interactive_shell_store()
            .expect("load store")
            .expect("store present");
        let loaded_snapshot = active_interactive_shell_snapshot(&loaded_store).expect("active snapshot");
        assert!(loaded_snapshot
            .messages
            .iter()
            .any(|message| message.content.contains("/model set to gpt-5-persist")));

        unsafe {
            std::env::remove_var("MULDEX_INTERACTIVE_SHELL_PATH");
        }
        cleanup(&path);
    }

    #[test]
    fn llm_router_config_commands_persist_user_config() {
        let _guard = config_env_lock().lock().expect("config env lock");
        let config_path = temp_root("muldex-cli-config.json");
        let _ = std::fs::remove_file(&config_path);
        unsafe {
            std::env::set_var("MULDEX_CONFIG_PATH", &config_path);
        }

        let message = handle_interactive_llm_config_command(
            InteractiveLlmConfigCommand::SetHost("127.0.0.1".to_string()),
        )
        .expect("set host");
        assert!(message.contains("llm-router host set to 127.0.0.1"));

        handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetPort(3000))
            .expect("set port");
        handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetApiKey("secret-key".to_string()))
            .expect("set api key");

        let config = load_muldex_config().expect("load config");
        let router = llm_router_provider(&config).expect("router config");
        assert_eq!(router.host.as_deref(), Some("127.0.0.1"));
        assert_eq!(router.port, Some(3000));
        assert_eq!(router.api_key.as_deref(), Some("secret-key"));

        unsafe {
            std::env::remove_var("MULDEX_CONFIG_PATH");
        }
        let _ = std::fs::remove_file(&config_path);
    }

    #[test]
    fn muldex_config_loads_manual_non_router_provider_entries() {
        let _guard = config_env_lock().lock().expect("config env lock");
        let config_path = temp_root("muldex-cli-manual-provider-config.json");
        let _ = std::fs::remove_file(&config_path);
        unsafe {
            std::env::set_var("MULDEX_CONFIG_PATH", &config_path);
        }

        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("openai-prod".to_string()),
            providers: std::collections::BTreeMap::from([(
                "openai-prod".to_string(),
                ProviderConfig {
                    kind: "openai-compatible".to_string(),
                    host: None,
                    port: None,
                    base_url: Some("https://api.openai.com/v1".to_string()),
                    api_key: None,
                    api_key_env: Some("OPENAI_API_KEY".to_string()),
                    default_model: Some("gpt-5".to_string()),
                },
            )]),
            llm_router: None,
        };
        save_muldex_config(&config).expect("save config");

        let loaded = load_muldex_config().expect("load config");
        assert_eq!(loaded.default_provider.as_deref(), Some("openai-prod"));
        let provider = loaded.providers.get("openai-prod").expect("provider present");
        assert_eq!(provider.base_url.as_deref(), Some("https://api.openai.com/v1"));
        assert_eq!(provider.api_key_env.as_deref(), Some("OPENAI_API_KEY"));

        unsafe {
            std::env::remove_var("MULDEX_CONFIG_PATH");
        }
        let _ = std::fs::remove_file(&config_path);
    }

    #[test]
    fn provider_use_switches_default_provider() {
        let _guard = config_env_lock().lock().expect("config env lock");
        let config_path = temp_root("muldex-cli-provider-switch.json");
        let _ = std::fs::remove_file(&config_path);
        unsafe {
            std::env::set_var("MULDEX_CONFIG_PATH", &config_path);
        }

        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("llm-router".to_string()),
            providers: std::collections::BTreeMap::from([
                (
                    "llm-router".to_string(),
                    ProviderConfig {
                        kind: "openai-compatible".to_string(),
                        host: Some("127.0.0.1".to_string()),
                        port: Some(3000),
                        base_url: None,
                        api_key: Some("abc".to_string()),
                        api_key_env: None,
                        default_model: None,
                    },
                ),
                (
                    "openai-prod".to_string(),
                    ProviderConfig {
                        kind: "openai-compatible".to_string(),
                        host: None,
                        port: None,
                        base_url: Some("https://api.openai.com/v1".to_string()),
                        api_key: None,
                        api_key_env: Some("OPENAI_API_KEY".to_string()),
                        default_model: Some("gpt-5".to_string()),
                    },
                ),
            ]),
            llm_router: None,
        };
        save_muldex_config(&config).expect("save config");

        let message = handle_interactive_provider_command(InteractiveProviderCommand::Use("openai-prod".to_string()))
            .expect("switch provider");
        assert!(message.contains("default provider set to openai-prod"));

        let loaded = load_muldex_config().expect("load config");
        assert_eq!(loaded.default_provider.as_deref(), Some("openai-prod"));

        unsafe {
            std::env::remove_var("MULDEX_CONFIG_PATH");
        }
        let _ = std::fs::remove_file(&config_path);
    }

    #[test]
    fn provider_test_reports_reachable_named_provider() {
        let _guard = config_env_lock().lock().expect("config env lock");
        let config_path = temp_root("muldex-cli-provider-test.json");
        let _ = std::fs::remove_file(&config_path);
        unsafe {
            std::env::set_var("MULDEX_CONFIG_PATH", &config_path);
        }

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let port = listener.local_addr().expect("local addr").port();
        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("openai-prod".to_string()),
            providers: std::collections::BTreeMap::from([(
                "openai-prod".to_string(),
                ProviderConfig {
                    kind: "openai-compatible".to_string(),
                    host: Some("127.0.0.1".to_string()),
                    port: Some(port),
                    base_url: None,
                    api_key: None,
                    api_key_env: Some("OPENAI_API_KEY".to_string()),
                    default_model: Some("gpt-5".to_string()),
                },
            )]),
            llm_router: None,
        };
        save_muldex_config(&config).expect("save config");

        let message = handle_interactive_provider_command(InteractiveProviderCommand::Test(Some(
            "openai-prod".to_string(),
        )))
        .expect("provider test");
        assert!(message.contains("provider test ok"));

        drop(listener);
        unsafe {
            std::env::remove_var("MULDEX_CONFIG_PATH");
        }
        let _ = std::fs::remove_file(&config_path);
    }

    #[test]
    fn llm_router_config_test_reports_missing_router_config() {
        let _guard = config_env_lock().lock().expect("config env lock");
        let config_path = temp_root("muldex-cli-config-missing.json");
        let _ = std::fs::remove_file(&config_path);
        unsafe {
            std::env::set_var("MULDEX_CONFIG_PATH", &config_path);
        }

        let message = handle_interactive_llm_config_command(InteractiveLlmConfigCommand::Test)
            .expect("run config test");
        assert!(message.contains("router config missing"));

        unsafe {
            std::env::remove_var("MULDEX_CONFIG_PATH");
        }
    }

    #[test]
    fn llm_router_config_test_reports_reachable_listener() {
        let _guard = config_env_lock().lock().expect("config env lock");
        let config_path = temp_root("muldex-cli-config-test.json");
        let _ = std::fs::remove_file(&config_path);
        unsafe {
            std::env::set_var("MULDEX_CONFIG_PATH", &config_path);
        }

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let port = listener.local_addr().expect("local addr").port();

        handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetHost("127.0.0.1".to_string()))
            .expect("set host");
        handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetPort(port))
            .expect("set port");

        let message = handle_interactive_llm_config_command(InteractiveLlmConfigCommand::Test)
            .expect("run config test");
        assert!(message.contains("test ok"));

        drop(listener);
        unsafe {
            std::env::remove_var("MULDEX_CONFIG_PATH");
        }
        let _ = std::fs::remove_file(&config_path);
    }

    #[test]
    fn interactive_prompt_buffer_supports_insert_move_and_backspace() {
        let mut buffer = InteractivePromptBuffer::default();
        buffer.insert_char('a');
        buffer.insert_char('c');
        buffer.move_left();
        buffer.insert_char('b');
        assert_eq!(buffer.text, "abc");
        assert_eq!(buffer.cursor, 2);

        buffer.backspace();
        assert_eq!(buffer.text, "ac");
        assert_eq!(buffer.cursor, 1);

        buffer.move_right();
        assert_eq!(buffer.cursor, 2);
        buffer.move_home();
        assert_eq!(buffer.cursor, 0);
        buffer.move_end();
        assert_eq!(buffer.cursor, 2);
        buffer.insert_newline();
        buffer.insert_char('z');
        assert_eq!(buffer.text, "ac\nz");
        buffer.clear();
        assert_eq!(buffer.text, "");
        assert_eq!(buffer.cursor, 0);
    }

    #[test]
    fn interactive_slash_hints_filter_from_first_line() {
        let mut buffer = InteractivePromptBuffer::default();
        buffer.text = "/re".to_string();
        buffer.cursor = buffer.text.len();
        let hints = filtered_interactive_slash_hints(&buffer);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].command, "/resume");

        buffer.text = "/mo\nsecond line".to_string();
        buffer.cursor = 3;
        let hints = filtered_interactive_slash_hints(&buffer);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].command, "/model");
    }

    #[test]
    fn interactive_slash_hint_lines_mark_active_row() {
        let mut buffer = InteractivePromptBuffer {
            text: "/".to_string(),
            cursor: 1,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        completion.update_from_buffer(&buffer);
        completion.select_next();

        let lines = render_interactive_slash_hint_lines(&buffer, &completion);
        assert_eq!(lines[0], "slash commands:");
        assert!(lines.iter().any(|line| line.starts_with("> /status - ")));
        assert!(lines.iter().any(|line| line.starts_with("  /help - ")));

        buffer.text = "hello".to_string();
        assert!(render_interactive_slash_hint_lines(&buffer, &completion).is_empty());
    }

    #[test]
    fn interactive_history_search_lines_show_query_and_match() {
        let history = InteractiveHistoryState::from_entries(vec!["alpha".to_string(), "beta".to_string()]);
        let mut search = InteractiveHistorySearchState::default();
        let mut buffer = InteractivePromptBuffer {
            text: "be".to_string(),
            cursor: 2,
        };
        assert!(search.reverse_search(&history, &mut buffer));

        let lines = render_interactive_history_search_lines(&history, &search);
        assert_eq!(lines[0], "reverse search active: be");
        assert_eq!(lines[1], "matches: 1");
        assert_eq!(lines[2], "match_index: 1/1");
        assert_eq!(lines[3], "match: beta");
    }

    #[test]
    fn interactive_history_search_lines_show_no_match_feedback() {
        let history = InteractiveHistoryState::from_entries(vec!["alpha".to_string()]);
        let mut search = InteractiveHistorySearchState::default();
        search.active = true;
        search.query = "zzz".to_string();

        let lines = render_interactive_history_search_lines(&history, &search);
        assert_eq!(lines[0], "reverse search active: zzz");
        assert_eq!(lines[1], "matches: 0");
        assert_eq!(lines[2], "match: none");
    }

    #[test]
    fn interactive_key_handler_submits_multiline_slash_buffer_as_prompt() {
        let mut buffer = InteractivePromptBuffer {
            text: "/model\nsecond line".to_string(),
            cursor: "/model\nsecond line".len(),
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(
            action,
            InteractiveKeyAction::Submit(InteractiveShellInput::Prompt(
                "/model\nsecond line".to_string()
            ))
        );
        assert_eq!(buffer.text, "");
    }

    #[test]
    fn interactive_key_handler_preserves_tab_for_non_slash_content() {
        let mut buffer = InteractivePromptBuffer {
            text: "hello".to_string(),
            cursor: 5,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::Noop);
        assert_eq!(buffer.text, "hello");
    }

    #[test]
    fn interactive_slash_picker_navigation_moves_selection() {
        let mut buffer = InteractivePromptBuffer {
            text: "/".to_string(),
            cursor: 1,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );
        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(completion.current_command(), Some("/status"));

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );
        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(completion.current_command(), Some("/help"));
    }

    #[test]
    fn interactive_slash_picker_first_escape_hides_picker_without_clearing_input() {
        let mut buffer = InteractivePromptBuffer {
            text: "/".to_string(),
            cursor: 1,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        completion.update_from_buffer(&buffer);
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(buffer.text, "/");
        assert!(!completion.visible);
    }

    #[test]
    fn interactive_slash_picker_second_escape_clears_input() {
        let mut buffer = InteractivePromptBuffer {
            text: "/".to_string(),
            cursor: 1,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        completion.update_from_buffer(&buffer);
        completion.visible = false;
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(buffer.text, "");
        assert!(!completion.visible);
    }

    #[test]
    fn interactive_slash_picker_tab_applies_selected_item() {
        let mut buffer = InteractivePromptBuffer {
            text: "/".to_string(),
            cursor: 1,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let _ = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );
        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(buffer.first_line(), "/status");
    }

    #[test]
    fn interactive_slash_picker_enter_applies_selected_item_before_submit() {
        let mut buffer = InteractivePromptBuffer {
            text: "/".to_string(),
            cursor: 1,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let _ = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );
        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(buffer.first_line(), "/status");
    }

    #[test]
    fn interactive_history_state_replays_and_restores_draft() {
        let mut history = InteractiveHistoryState::from_entries(vec![
            "first".to_string(),
            "second".to_string(),
        ]);
        let mut buffer = InteractivePromptBuffer {
            text: "draft".to_string(),
            cursor: 5,
        };

        assert!(history.previous(&mut buffer));
        assert_eq!(buffer.text, "second");
        assert!(history.previous(&mut buffer));
        assert_eq!(buffer.text, "first");
        assert!(history.next(&mut buffer));
        assert_eq!(buffer.text, "second");
        assert!(history.next(&mut buffer));
        assert_eq!(buffer.text, "draft");
    }

    #[test]
    fn interactive_history_state_dedupes_consecutive_submissions() {
        let mut history = InteractiveHistoryState::default();
        history.record_submission("same");
        history.record_submission("same");
        history.record_submission("  ");
        history.record_submission("next");
        history.record_submission("next");

        assert_eq!(history.entries, vec!["same".to_string(), "next".to_string()]);
    }

    #[test]
    fn interactive_history_search_finds_and_cycles_matches() {
        let history = InteractiveHistoryState::from_entries(vec![
            "alpha task".to_string(),
            "beta fix".to_string(),
            "alpha review".to_string(),
        ]);
        let mut search = InteractiveHistorySearchState::default();
        let mut buffer = InteractivePromptBuffer {
            text: "alpha".to_string(),
            cursor: 5,
        };

        assert!(search.reverse_search(&history, &mut buffer));
        assert_eq!(buffer.text, "alpha review");
        assert!(search.reverse_search(&history, &mut buffer));
        assert_eq!(buffer.text, "alpha task");
    }

    #[test]
    fn interactive_history_search_extends_and_restores_draft() {
        let history = InteractiveHistoryState::from_entries(vec![
            "first fix".to_string(),
            "second fix".to_string(),
        ]);
        let mut search = InteractiveHistorySearchState::default();
        let mut buffer = InteractivePromptBuffer {
            text: "fi".to_string(),
            cursor: 2,
        };

        assert!(search.reverse_search(&history, &mut buffer));
        assert_eq!(buffer.text, "second fix");
        assert!(search.extend_query(&history, &mut buffer, 'x'));
        assert_eq!(buffer.text, "second fix");
        assert_eq!(search.query, "fix");
        assert!(search.restore_draft(&mut buffer));
        assert_eq!(buffer.text, "fi");
        assert!(!search.is_active());
    }

    #[test]
    fn interactive_prompt_buffer_supports_word_motion_and_delete_word() {
        let mut buffer = InteractivePromptBuffer {
            text: "alpha beta gamma".to_string(),
            cursor: "alpha beta gamma".len(),
        };

        buffer.move_word_left();
        assert_eq!(buffer.cursor, "alpha beta ".len());
        buffer.move_word_left();
        assert_eq!(buffer.cursor, "alpha ".len());
        buffer.move_word_right();
        assert_eq!(buffer.cursor, "alpha beta ".len());
        buffer.delete_word_left();
        assert_eq!(buffer.text, "alpha gamma");
    }

    #[test]
    fn interactive_key_handler_uses_history_when_not_in_slash_mode() {
        let mut buffer = InteractivePromptBuffer {
            text: "draft".to_string(),
            cursor: 5,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::from_entries(vec!["past".to_string()]);
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawPrompt);
        assert_eq!(buffer.text, "past");
    }

    #[test]
    fn interactive_key_handler_supports_reverse_history_search() {
        let mut buffer = InteractivePromptBuffer {
            text: "fix".to_string(),
            cursor: 3,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::from_entries(vec![
            "first fix".to_string(),
            "second fix".to_string(),
        ]);
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(buffer.text, "second fix");
    }

    #[test]
    fn interactive_key_handler_escape_restores_reverse_search_draft() {
        let mut buffer = InteractivePromptBuffer {
            text: "fix".to_string(),
            cursor: 3,
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::from_entries(vec!["second fix".to_string()]);
        let mut search = InteractiveHistorySearchState::default();

        let _ = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );
        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(buffer.text, "fix");
        assert!(!search.is_active());
    }

    #[test]
    fn interactive_key_handler_supports_word_shortcuts() {
        let mut buffer = InteractivePromptBuffer {
            text: "alpha beta".to_string(),
            cursor: "alpha beta".len(),
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Left, KeyModifiers::ALT),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );
        assert_eq!(action, InteractiveKeyAction::RedrawPrompt);
        assert_eq!(buffer.cursor, "alpha ".len());

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
        );
        assert_eq!(action, InteractiveKeyAction::RedrawPrompt);
        assert_eq!(buffer.text, "beta");
    }

    #[test]
    fn interactive_slash_completion_no_match_is_ignored() {
        let mut buffer = InteractivePromptBuffer {
            text: "/zzz".to_string(),
            cursor: 4,
        };
        let mut completion = InteractiveSlashCompletionState::default();

        assert!(!apply_interactive_slash_completion(&mut buffer, &mut completion));
        assert_eq!(buffer.text, "/zzz");
        assert!(completion.matches.is_empty());
    }

    #[test]
    fn interactive_slash_completion_single_match_completes_command() {
        let mut buffer = InteractivePromptBuffer {
            text: "/re".to_string(),
            cursor: 3,
        };
        let mut completion = InteractiveSlashCompletionState::default();

        assert!(apply_interactive_slash_completion(&mut buffer, &mut completion));
        assert_eq!(buffer.text, "/resume");
        assert_eq!(buffer.cursor, "/resume".len());
        assert_eq!(completion.matches, vec!["/resume"]);
    }

    #[test]
    fn interactive_slash_completion_cycles_multiple_matches() {
        let mut buffer = InteractivePromptBuffer {
            text: "/".to_string(),
            cursor: 1,
        };
        let mut completion = InteractiveSlashCompletionState::default();

        assert!(apply_interactive_slash_completion(&mut buffer, &mut completion));
        assert_eq!(buffer.text, "/help");

        assert!(apply_interactive_slash_completion(&mut buffer, &mut completion));
        assert_eq!(buffer.text, "/status");

        assert!(apply_interactive_slash_completion(&mut buffer, &mut completion));
        assert_eq!(buffer.text, "/model");
    }

    #[test]
    fn interactive_slash_completion_only_replaces_first_line() {
        let mut buffer = InteractivePromptBuffer {
            text: "/mo\nsecond line".to_string(),
            cursor: "/mo\nsecond line".len(),
        };
        let mut completion = InteractiveSlashCompletionState::default();

        assert!(apply_interactive_slash_completion(&mut buffer, &mut completion));
        assert_eq!(buffer.text, "/model\nsecond line");
        assert_eq!(buffer.cursor, "/model\nsecond line".len());
    }

    #[test]
    fn interactive_slash_completion_ignores_non_slash_content() {
        let mut buffer = InteractivePromptBuffer {
            text: "plain input".to_string(),
            cursor: "plain input".len(),
        };
        let mut completion = InteractiveSlashCompletionState::default();

        assert!(!apply_interactive_slash_completion(&mut buffer, &mut completion));
        assert_eq!(buffer.text, "plain input");
        assert_eq!(buffer.cursor, "plain input".len());
    }

    #[test]
    fn client_list_sessions_helper_returns_session_view() {
        let path = temp_root("muldex-cli-list-sessions.json");
        cleanup(&path);

        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-list-1", sample_state())
            .expect("create session");
        daemon.save().expect("save daemon");

        let view = load_client_session_list_view(path.clone()).expect("load session list view");
        assert_eq!(view.contract.schema_version, "client-view-v1");
        assert_eq!(view.session_count, 1);
        assert_eq!(view.sessions[0].session_id, "session-list-1");

        daemon.shutdown().expect("shutdown daemon");
        cleanup(&path);
    }

    #[test]
    fn client_inspect_session_helper_returns_raw_report_view() {
        let path = temp_root("muldex-cli-inspect-session.json");
        cleanup(&path);

        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-inspect-1", sample_state())
            .expect("create session");
        daemon
            .host_mut()
            .expect("host")
            .apply_command(
                "session-inspect-1",
                muldex_runtime::runtime::RuntimeCommand::Decision(muldex_core::protocol::ContinueDecision {
                    allow_continue: true,
                    mode: muldex_core::protocol::ContinueMode::NextTurn,
                    rationale: "seed report for inspect".to_string(),
                    next_action: None,
                    pause_for_approval: false,
                    consume_interrupts_now: false,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                }),
            )
            .expect("apply command");
        daemon.save().expect("save daemon");

        let view = load_client_inspect_session_view(
            path.clone(),
            "session-inspect-1".to_string(),
            SessionExportModeArg::Raw,
        )
        .expect("load inspect view");

        assert_eq!(view.session_id, "session-inspect-1");
        assert_eq!(view.phase, RuntimePhase::Running);
        match view.report.expect("report") {
            ExportedReportView::Raw(report) => {
                assert_eq!(report.rationale, "seed report for inspect");
            }
            _ => panic!("expected raw report view"),
        }

        daemon.shutdown().expect("shutdown daemon");
        cleanup(&path);
    }

    #[test]
    fn client_inspect_session_helper_returns_compressed_report_view() {
        let path = temp_root("muldex-cli-inspect-session-compressed.json");
        cleanup(&path);

        let mut daemon = RuntimeDaemon::new(&path);
        daemon.boot_empty().expect("boot daemon");
        daemon
            .host_mut()
            .expect("host")
            .create_session("session-inspect-2", sample_state())
            .expect("create session");
        daemon
            .host_mut()
            .expect("host")
            .apply_command(
                "session-inspect-2",
                muldex_runtime::runtime::RuntimeCommand::Decision(muldex_core::protocol::ContinueDecision {
                    allow_continue: true,
                    mode: muldex_core::protocol::ContinueMode::NextTurn,
                    rationale: "seed report for compressed inspect".to_string(),
                    next_action: None,
                    pause_for_approval: false,
                    consume_interrupts_now: false,
                    may_continue_other_work: true,
                    suppress_duplicate_injection: false,
                    downgrade_trigger_turn: false,
                    request_compaction: false,
                    request_handoff_summary: false,
                    request_checkpoint: false,
                    enter_self_correction: false,
                }),
            )
            .expect("apply command");
        daemon.save().expect("save daemon");

        let view = load_client_inspect_session_view(
            path.clone(),
            "session-inspect-2".to_string(),
            SessionExportModeArg::Compressed,
        )
        .expect("load inspect view");

        assert_eq!(view.session_id, "session-inspect-2");
        match view.report.expect("report") {
            ExportedReportView::Compressed(report) => {
                assert_eq!(report.rationale, "seed report for compressed inspect");
                assert!(report.compressed_cycle_summary.is_some());
            }
            _ => panic!("expected compressed report view"),
        }

        daemon.shutdown().expect("shutdown daemon");
        cleanup(&path);
    }
}

fn print_bootstrap_snapshot_summary(snapshot: &CodexBootstrapSnapshot) {
    println!("snapshot.kind: codex-bootstrap");
    println!("snapshot.model: {}", snapshot.model);
    println!("snapshot.provider: {}", snapshot.model_provider);
    println!("snapshot.mode: {}", snapshot.collaboration_mode);
    println!("snapshot.personality: {:?}", snapshot.personality);
    println!("snapshot.approval_policy: {}", snapshot.approval_policy);
    println!("snapshot.service_tier: {:?}", snapshot.service_tier);
    println!("snapshot.show_raw_agent_reasoning: {}", snapshot.show_raw_agent_reasoning);
    println!("snapshot.reference_context: {}", snapshot.reference_context_present);
    println!("snapshot.input_modalities: {:?}", snapshot.input_modalities);
    println!("snapshot.tools_visible: {}", snapshot.tools_visible_count);
}

fn print_live_snapshot_summary(snapshot: &CodexLiveContinuationSnapshot) {
    println!("snapshot.kind: codex-live");
    println!("snapshot.thread_id: {}", snapshot.thread_id);
    println!("snapshot.active_turn_present: {}", snapshot.active_turn_present);
    println!("snapshot.pending_input_present: {}", snapshot.pending_input_present);
    println!(
        "snapshot.trigger_turn_mailbox_present: {}",
        snapshot.trigger_turn_mailbox_present
    );
    println!(
        "snapshot.auto_compact_window_number: {}",
        snapshot.auto_compact_window_number
    );
    println!("snapshot.total_input_tokens: {:?}", snapshot.total_input_tokens);
}

fn build_workspace_request(
    workspace: PathBuf,
    objective: Option<String>,
    objective_file: Option<PathBuf>,
    mode: String,
    no_progress_iterations: u32,
    post_compaction: bool,
    recoverable_failure: bool,
) -> Result<ReasoningHarnessRequest, Box<dyn std::error::Error>> {
    if !workspace.exists() || !workspace.is_dir() {
        return Err(format!("workspace does not exist or is not a directory: {}", workspace.display()).into());
    }

    let objective = match (objective, objective_file) {
        (Some(text), None) => text,
        (None, Some(path)) => fs::read_to_string(path)?,
        (Some(_), Some(_)) => {
            return Err("provide either --objective or --objective-file, not both".into())
        }
        (None, None) => return Err("provide --objective or --objective-file".into()),
    };

    let git_hint = if workspace.join(".git").exists() {
        "git repository"
    } else {
        "non-git workspace"
    };

    let mut request = sample_request(Scenario::Healthy);
    request.objective = objective.trim().to_string();
    request.evidence_scope = vec![
        format!("workspace: {}", workspace.display()),
        format!("workspace_kind: {git_hint}"),
    ];
    request.runtime_mode.active_agent_mode = Some(mode);
    request.progress.no_progress_iteration_count = no_progress_iterations;

    if post_compaction {
        request.post_compaction.pending_post_compaction = true;
        request.post_compaction.first_post_compaction_turn = true;
    }

    if recoverable_failure {
        request.recovery.last_recovery_reason = Some(RecoveryReason::ToolFailure);
        request.recovery.last_recovery_had_progress = false;
        request.self_correction.active = true;
        request.self_correction.correction_attempt_count = 1;
        request.self_correction.last_correction_target = Some(
            "retry failed step in real workspace".to_string(),
        );
    }

    Ok(request)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        None => {
            run_interactive_shell(cli.prompt)?;
        }
        Some(Command::DecideSample { scenario }) => {
            let request = sample_request(scenario);
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            let runtime_result = apply_runtime_step(request);
            print_runtime_step_result(&runtime_result);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Some(Command::DecideFile { path }) => {
            let raw = fs::read_to_string(path)?;
            let request: ReasoningHarnessRequest = serde_json::from_str(&raw)?;
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            let runtime_result = apply_runtime_step(request);
            print_runtime_step_result(&runtime_result);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Some(Command::DecideCodexSnapshot { path }) => {
            let raw = fs::read_to_string(path)?;
            let request = match serde_json::from_str::<CodexSignalSnapshot>(&raw) {
                Ok(snapshot) => codex_snapshot_to_harness_request(snapshot),
                Err(_) => match serde_json::from_str::<CodexLiveContinuationSnapshot>(&raw) {
                    Ok(live) => {
                        print_live_snapshot_summary(&live);
                        println!();
                        codex_live_snapshot_to_harness_request(live)
                    }
                    Err(_) => {
                        let bootstrap: CodexBootstrapSnapshot = serde_json::from_str(&raw)?;
                        print_bootstrap_snapshot_summary(&bootstrap);
                        println!();
                        codex_bootstrap_snapshot_to_harness_request(bootstrap)
                    }
                }
            };
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            let runtime_result = apply_runtime_step(request);
            print_runtime_step_result(&runtime_result);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Some(Command::DecideWorkspace {
            workspace,
            objective,
            objective_file,
            mode,
            no_progress_iterations,
            post_compaction,
            recoverable_failure,
            print_request,
        }) => {
            let request = build_workspace_request(
                workspace,
                objective,
                objective_file,
                mode,
                no_progress_iterations,
                post_compaction,
                recoverable_failure,
            )?;
            if print_request {
                println!("{}", serde_json::to_string_pretty(&request)?);
                println!();
            }
            let decision = decide_reasoning_harness(&request);
            print_decision(&decision);
            println!();
            let runtime_result = apply_runtime_step(request);
            print_runtime_step_result(&runtime_result);
            println!();
            println!("{}", serde_json::to_string_pretty(&decision)?);
        }
        Some(Command::DemoApprovalResume) => {
            demo_approval_resume();
        }
        Some(Command::DemoHostPersistence) => {
            demo_host_persistence()?;
        }
        Some(Command::SaveHostSnapshot { path }) => {
            save_host_snapshot(path)?;
        }
        Some(Command::LoadHostSnapshot { path }) => {
            load_host_snapshot(path)?;
        }
        Some(Command::ImportCodexSnapshot { path }) => {
            import_codex_snapshot(path)?;
        }
        Some(Command::ExportSessionView {
            path,
            session_id,
            mode,
        }) => {
            export_session_view_command(path, session_id, mode)?;
        }
        Some(Command::DaemonBootEmpty { path }) => {
            daemon_boot_empty(path);
        }
        Some(Command::DaemonBootLoad { path }) => {
            daemon_boot_load(path)?;
        }
        Some(Command::DaemonSave { path }) => {
            daemon_save(path)?;
        }
        Some(Command::DaemonStatus { path }) => {
            daemon_status(path)?;
        }
        Some(Command::DaemonServeOnce { path }) => {
            daemon_serve_once(path)?;
        }
        Some(Command::DaemonServeLoop { path, iterations }) => {
            daemon_serve_loop(path, iterations)?;
        }
        Some(Command::DaemonSendCommand {
            path,
            command_id,
            session_id,
            kind,
        }) => {
            daemon_send_command(path, command_id, session_id, kind)?;
        }
        Some(Command::DaemonReadResponse { path, command_id }) => {
            daemon_read_response(path, command_id)?;
        }
        Some(Command::DaemonStaleStatus { path, threshold_ms }) => {
            daemon_stale_status(path, threshold_ms)?;
        }
        Some(Command::DaemonForceTakeover { path, threshold_ms }) => {
            daemon_force_takeover(path, threshold_ms)?;
        }
        Some(Command::ServerForeground { path, iterations }) => {
            server_foreground(path, iterations)?;
        }
        Some(Command::ClientStatus { path }) => {
            client_status(path)?;
        }
        Some(Command::ClientSendCommand {
            path,
            command_id,
            session_id,
            kind,
            access_mode,
        }) => {
            client_send_command(path, command_id, session_id, kind, access_mode)?;
        }
        Some(Command::ClientReadResponse { path, command_id }) => {
            client_read_response(path, command_id)?;
        }
        Some(Command::ClientListSessions { path }) => {
            client_list_sessions(path)?;
        }
        Some(Command::ClientInspectSession {
            path,
            session_id,
            mode,
        }) => {
            client_inspect_session(path, session_id, mode)?;
        }
        Some(Command::ClientExportSession { path, session_id }) => {
            client_export_session(path, session_id)?;
        }
    }

    let _ = ContinueMode::NextTurn;
    Ok(())
}
