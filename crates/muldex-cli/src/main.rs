use clap::Parser;
use clap::Subcommand;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use muldex_core::provider::MuldexConfig;
use muldex_core::provider::ProviderConfig;
use muldex_core::provider::ProviderMessageRole;
use muldex_core::provider::ProviderTurnMessage;
use muldex_core::provider::resolve_provider_config;
use notify::{RecommendedWatcher, Watcher, Config as NotifyConfig, Event as NotifyEvent, EventKind};
use muldex_core::protocol::ApprovalPolicyDescriptor;
use muldex_core::protocol::CapabilityRegistrySnapshot;
use muldex_core::protocol::CheckpointRef;
use muldex_core::protocol::CodexSessionContinuationSnapshot;
use muldex_core::protocol::ContextPressure;
use muldex_core::protocol::ContinueDecision;
use muldex_core::protocol::ContinueMode;
use muldex_core::protocol::ContinueReason;
use muldex_core::protocol::ContinueRequest;
use muldex_core::protocol::CycleSummary;
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
use muldex_core::protocol::PermissionContextSnapshot;
use muldex_core::protocol::PermissionDecision;
use muldex_core::protocol::PermissionDecisionStatus;
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
use muldex_runtime::client_policy::ClientAccessMode;
use muldex_runtime::client_policy::client_command_allowed;
use muldex_runtime::client_views::ClientCommandView;
use muldex_runtime::client_views::command_envelope_view;
use muldex_runtime::client_views::command_receipt_view;
use muldex_runtime::client_views::daemon_status_view;
use muldex_runtime::client_views::inspect_session_view;
use muldex_runtime::client_views::project_client_command;
use muldex_runtime::client_views::response_view;
use muldex_runtime::client_views::session_list_view;
use muldex_runtime::continuity::ExternalRuntimeSnapshot;
use muldex_runtime::continuity::ReportExportMode;
use muldex_runtime::continuity::export_host;
use muldex_runtime::continuity::export_session;
use muldex_runtime::continuity::export_session_view;
use muldex_runtime::continuity::import_external_snapshot_as_runtime_state;
use muldex_runtime::daemon::RuntimeDaemon;
use muldex_runtime::daemon_local::StaleOwnershipStatus;
use muldex_runtime::daemon_transport::DaemonCommandEnvelope;
use muldex_runtime::daemon_transport::FileCommandTransport;
use muldex_runtime::host::RuntimeHost;
use muldex_runtime::interactive_turn::InteractiveToolError;
use muldex_runtime::interactive_turn::InteractiveToolExecutor;
use muldex_runtime::interactive_turn::TurnExecutionRequest;
use muldex_runtime::interactive_turn::UiEventListener;
use muldex_runtime::interactive_turn::execute_interactive_turn;
use muldex_runtime::responses_provider::ResponsesProvider;
use muldex_runtime::ui_events::UiEvent;
use muldex_runtime::runtime::RuntimeCommand;
use muldex_runtime::runtime::RuntimeCommandResult;
use muldex_runtime::runtime::RuntimeDriveResult;
use muldex_runtime::runtime::RuntimeDriver;
use muldex_runtime::runtime::RuntimeEvent;
use muldex_runtime::runtime::RuntimePhase;
use muldex_runtime::runtime::RuntimeState;
use muldex_runtime::runtime::RuntimeStepResult;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::net::TcpStream;
use std::net::ToSocketAddrs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

mod interactive_tui;

static KEYMAP: RwLock<Option<Arc<interactive_tui::RuntimeKeymap>>> = RwLock::new(None);

fn init_keymap() -> Arc<interactive_tui::RuntimeKeymap> {
    let mut km = interactive_tui::RuntimeKeymap::defaults();
    if let Err(e) = km.validate() {
        eprintln!("keymap validation: {e}");
    }
    if let Some(config) = config::MuldexConfig::load() {
        config.apply_keymap(&mut km);
    }
    let arc = Arc::new(km);
    {
        let mut guard = KEYMAP.write().unwrap();
        *guard = Some(arc.clone());
    }
    arc
}

fn get_keymap() -> Arc<interactive_tui::RuntimeKeymap> {
    KEYMAP.read().unwrap().as_ref().unwrap().clone()
}

fn reload_keymap() {
    let mut km = interactive_tui::RuntimeKeymap::defaults();
    if let Err(e) = km.validate() {
        eprintln!("keymap validation: {e}");
    }
    if let Some(config) = config::MuldexConfig::load() {
        config.apply_keymap(&mut km);
    }
    let arc = Arc::new(km);
    let mut guard = KEYMAP.write().unwrap();
    *guard = Some(arc);
    eprintln!("keymap reloaded from config");
}

fn start_config_watcher() {
    let (tx, rx) = std::sync::mpsc::channel::<notify::Event>();
    let mut watcher = match RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        },
        NotifyConfig::default(),
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("failed to create config watcher: {e}");
            return;
        }
    };
    
    if let Some(path) = config::config_path() {
        if let Some(parent) = path.parent() {
            if watcher.watch(parent, notify::RecursiveMode::NonRecursive).is_ok() {
                std::thread::spawn(move || {
                    while let Ok(event) = rx.recv() {
                        match event.kind {
                            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                                if event.paths.iter().any(|p| p.file_name().map(|n| n == "config.toml").unwrap_or(false)) {
                                    std::thread::sleep(Duration::from_millis(50));
                                    reload_keymap();
                                }
                            }
                            _ => {}
                        }
                    }
                });
                return;
            }
        }
    }
    eprintln!("config hot-reload disabled: could not watch config directory");
}

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
    #[serde(default)]
    usage: ShellUsage,
    #[serde(default)]
    rate_limit: ShellRateLimit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct ShellUsage {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct ShellRateLimit {
    limit_requests: Option<u64>,
    remaining_requests: Option<u64>,
    limit_tokens: Option<u64>,
    remaining_tokens: Option<u64>,
    reset_after_seconds: Option<u64>,
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
    ClearScreen,
    OpenExternalEditor,
    Copy,
    Yank,
    Submit(InteractiveShellInput),
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

    fn row_col(&self) -> (usize, usize) {
        let before = &self.text[..self.cursor];
        let row = before.lines().count().saturating_sub(1);
        let col = before.lines().last().map(|l| l.chars().count()).unwrap_or(0);
        (row, col)
    }

    fn line_count(&self) -> usize {
        if self.text.is_empty() {
            1
        } else {
            self.text.lines().count()
        }
    }

    fn line_at(&self, row: usize) -> &str {
        self.text.lines().nth(row).unwrap_or("")
    }

    fn set_row_col(&mut self, row: usize, col: usize) {
        let total = self.line_count();
        let row = row.min(total.saturating_sub(1));
        let line = self.line_at(row);
        let col = col.min(line.chars().count());
        let mut offset = 0;
        for (i, l) in self.text.lines().enumerate() {
            if i == row {
                let byte = line.char_indices().nth(col).map(|(b, _)| b).unwrap_or(line.len());
                offset += byte;
                break;
            }
            offset += l.len() + 1;
        }
        self.cursor = offset.min(self.text.len());
    }
}

// ---- Vim modal editing + kill/yank ring (opt-in via MULDEX_VIM=on) ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum VimOp {
    Delete,
    Yank,
}

#[derive(Default)]
struct VimState {
    enabled: bool,
    normal: bool,
    pending: Option<VimOp>,
    ring: Vec<String>,
}

impl VimState {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            normal: enabled,
            pending: None,
            ring: Vec::new(),
        }
    }
}

pub(crate) fn vim_state() -> &'static Mutex<VimState> {
    static VIM: OnceLock<Mutex<VimState>> = OnceLock::new();
    VIM.get_or_init(|| {
        Mutex::new(VimState::new(
            std::env::var("MULDEX_VIM").as_deref() == Ok("on"),
        ))
    })
}

fn vim_word_start(text: &str, from: usize) -> usize {
    let rest = &text[from..];
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    from + i.min(rest.len())
}

fn vim_word_end(text: &str, from: usize) -> usize {
    let rest = &text[from..];
    let bytes = rest.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return text.len();
    }
    while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    from + i.min(rest.len())
}

fn vim_word_back(text: &str, from: usize) -> usize {
    if from == 0 {
        return 0;
    }
    let prefix = &text[..from];
    let bytes = prefix.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    while i > 0 && !bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    i
}

fn vim_line_bounds(buffer: &InteractivePromptBuffer, row: usize) -> (usize, usize) {
    let mut start = 0;
    for (idx, l) in buffer.text.lines().enumerate() {
        if idx == row {
            break;
        }
        start += l.len() + 1;
    }
    let line = buffer.line_at(row);
    let mut end = start + line.len();
    let bytes = buffer.text.as_bytes();
    if end < bytes.len() && bytes[end] == b'\n' {
        end += 1;
    } else if start > 0 && bytes[start - 1] == b'\n' {
        return (start - 1, end);
    }
    (start, end)
}

fn vim_delete_range(buffer: &mut InteractivePromptBuffer, range: std::ops::Range<usize>, st: &mut VimState) {
    if range.start < range.end && range.end <= buffer.text.len() {
        let captured: String = buffer.text[range.clone()].to_string();
        st.ring.push(captured);
        if st.ring.len() > 16 {
            st.ring.remove(0);
        }
        buffer.text.drain(range.clone());
        buffer.cursor = range.start.min(buffer.text.len());
    }
}

fn vim_yank_range(buffer: &InteractivePromptBuffer, range: std::ops::Range<usize>, st: &mut VimState) {
    if range.end <= buffer.text.len() {
        let captured: String = buffer.text[range].to_string();
        st.ring.push(captured);
        if st.ring.len() > 16 {
            st.ring.remove(0);
        }
    }
}

fn vim_put(buffer: &mut InteractivePromptBuffer, st: &VimState, after: bool) {
    if let Some(text) = st.ring.last() {
        if after {
            buffer.text.insert_str(buffer.cursor, text);
            buffer.cursor = (buffer.cursor + text.len()).min(buffer.text.len());
        } else {
            buffer.text.insert_str(buffer.cursor, text);
            // cursor stays at insertion start
        }
    }
}

fn vim_normal_key(
    st: &mut VimState,
    key: KeyEvent,
    buffer: &mut InteractivePromptBuffer,
) -> Option<InteractiveKeyAction> {
    use KeyCode::*;
    let action = match key.code {
        Char('i') => {
            st.normal = false;
            InteractiveKeyAction::RedrawFrame
        }
        Char('a') => {
            buffer.move_right();
            st.normal = false;
            InteractiveKeyAction::RedrawFrame
        }
        Char('A') => {
            let (r, _) = buffer.row_col();
            buffer.set_row_col(r, buffer.line_at(r).chars().count());
            st.normal = false;
            InteractiveKeyAction::RedrawFrame
        }
        Char('I') => {
            let (r, _) = buffer.row_col();
            buffer.set_row_col(r, 0);
            st.normal = false;
            InteractiveKeyAction::RedrawFrame
        }
        Char('o') => {
            let (r, _) = buffer.row_col();
            buffer.set_row_col(r, buffer.line_at(r).chars().count());
            buffer.insert_char('\n');
            st.normal = false;
            InteractiveKeyAction::RedrawFrame
        }
        Char('O') => {
            let (r, _) = buffer.row_col();
            buffer.set_row_col(r, 0);
            buffer.insert_char('\n');
            let (r2, _) = buffer.row_col();
            buffer.set_row_col(r2.saturating_sub(1), 0);
            st.normal = false;
            InteractiveKeyAction::RedrawFrame
        }
        Char('h') => {
            buffer.move_left();
            InteractiveKeyAction::RedrawFrame
        }
        Char('l') => {
            buffer.move_right();
            InteractiveKeyAction::RedrawFrame
        }
        Char('j') => {
            let (r, c) = buffer.row_col();
            buffer.set_row_col(r + 1, c);
            InteractiveKeyAction::RedrawFrame
        }
        Char('k') => {
            let (r, c) = buffer.row_col();
            buffer.set_row_col(r.saturating_sub(1), c);
            InteractiveKeyAction::RedrawFrame
        }
        Char('w') => {
            if st.pending == Some(VimOp::Delete) {
                let end = vim_word_start(&buffer.text, buffer.cursor);
                vim_delete_range(buffer, buffer.cursor..end, st);
                st.pending = None;
            } else if st.pending == Some(VimOp::Yank) {
                let end = vim_word_start(&buffer.text, buffer.cursor);
                vim_yank_range(buffer, buffer.cursor..end, st);
                st.pending = None;
            } else {
                buffer.cursor = vim_word_start(&buffer.text, buffer.cursor).min(buffer.text.len());
            }
            InteractiveKeyAction::RedrawFrame
        }
        Char('e') => {
            if st.pending == Some(VimOp::Delete) {
                let end = vim_word_end(&buffer.text, buffer.cursor);
                vim_delete_range(buffer, buffer.cursor..end, st);
                st.pending = None;
            } else if st.pending == Some(VimOp::Yank) {
                let end = vim_word_end(&buffer.text, buffer.cursor);
                vim_yank_range(buffer, buffer.cursor..end, st);
                st.pending = None;
            } else {
                buffer.cursor = vim_word_end(&buffer.text, buffer.cursor).min(buffer.text.len());
            }
            InteractiveKeyAction::RedrawFrame
        }
        Char('b') => {
            if st.pending == Some(VimOp::Delete) {
                let start = vim_word_back(&buffer.text, buffer.cursor);
                vim_delete_range(buffer, start..buffer.cursor, st);
                st.pending = None;
            } else if st.pending == Some(VimOp::Yank) {
                let start = vim_word_back(&buffer.text, buffer.cursor);
                vim_yank_range(buffer, start..buffer.cursor, st);
                st.pending = None;
            } else {
                buffer.cursor = vim_word_back(&buffer.text, buffer.cursor);
            }
            InteractiveKeyAction::RedrawFrame
        }
        Char('0') => {
            let (r, _) = buffer.row_col();
            buffer.set_row_col(r, 0);
            InteractiveKeyAction::RedrawFrame
        }
        Char('$') => {
            let (r, _) = buffer.row_col();
            buffer.set_row_col(r, buffer.line_at(r).chars().count());
            InteractiveKeyAction::RedrawFrame
        }
        Char('G') => {
            buffer.cursor = buffer.text.len();
            InteractiveKeyAction::RedrawFrame
        }
        Char('x') => {
            vim_delete_range(buffer, buffer.cursor..buffer.cursor.saturating_add(1), st);
            InteractiveKeyAction::RedrawFrame
        }
        Char('X') => {
            let start = buffer.cursor.saturating_sub(1);
            vim_delete_range(buffer, start..buffer.cursor, st);
            InteractiveKeyAction::RedrawFrame
        }
        Char('d') => {
            if st.pending == Some(VimOp::Delete) {
                let (s, e) = vim_line_bounds(buffer, buffer.row_col().0);
                vim_delete_range(buffer, s..e, st);
                st.pending = None;
            } else {
                st.pending = Some(VimOp::Delete);
            }
            InteractiveKeyAction::RedrawFrame
        }
        Char('y') => {
            if st.pending == Some(VimOp::Yank) {
                let (s, e) = vim_line_bounds(buffer, buffer.row_col().0);
                let line_len = buffer.line_at(buffer.row_col().0).chars().count();
                vim_yank_range(buffer, s..s + line_len, st);
                st.pending = None;
            } else {
                st.pending = Some(VimOp::Yank);
            }
            InteractiveKeyAction::RedrawFrame
        }
        Char('p') => {
            vim_put(buffer, st, true);
            InteractiveKeyAction::RedrawFrame
        }
        Char('P') => {
            vim_put(buffer, st, false);
            InteractiveKeyAction::RedrawFrame
        }
        Esc => {
            st.normal = true;
            st.pending = None;
            InteractiveKeyAction::RedrawFrame
        }
        _ => InteractiveKeyAction::Noop,
    };
    Some(action)
}

/// Route a key through Vim modal editing when enabled. Returns `None` to let
/// the default composer handling take over (used by insert mode typing).
fn vim_handle_key(key: KeyEvent, buffer: &mut InteractivePromptBuffer) -> Option<InteractiveKeyAction> {
    let mut st = vim_state().lock().unwrap();
    if !st.enabled {
        return None;
    }
    if st.normal {
        vim_normal_key(&mut st, key, buffer)
    } else if matches!(key.code, KeyCode::Esc) {
        st.normal = true;
        st.pending = None;
        Some(InteractiveKeyAction::RedrawFrame)
    } else {
        None
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
        let next_index = self
            .index_from_end
            .unwrap_or(0)
            .saturating_add(1)
            .min(self.entries.len());
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
    DecideFile {
        path: PathBuf,
    },
    DecideCodexSnapshot {
        path: PathBuf,
    },
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
                summary: "validated recent progress and left a safe-point interrupt queued"
                    .to_string(),
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
            request.self_correction.last_correction_target =
                Some("retry failed tool step".to_string());
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
    println!(
        "mode: {}",
        match decision.mode {
            ContinueMode::SameTurn => "same_turn",
            ContinueMode::NextTurn => "next_turn",
            ContinueMode::QueueOnly => "queue_only",
            ContinueMode::Handoff => "handoff",
            ContinueMode::Stop => "stop",
        }
    );
    println!("checkpoint: {}", decision.should_checkpoint);
    println!("self_correction: {}", decision.should_enter_self_correction);
    println!("pause_for_approval: {}", decision.pause_for_approval);
    println!(
        "consume_interrupts_now: {}",
        decision.consume_interrupts_now
    );
    println!(
        "may_continue_other_work: {}",
        decision.may_continue_other_work
    );
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
    println!(
        "runtime.final_cycle_index: {}",
        result.final_state.cycle_index
    );
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
        allow_continue: !matches!(
            harness_decision.mode,
            ContinueMode::Handoff | ContinueMode::Stop
        ),
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
        other => panic!(
            "unexpected runtime command result for decision: {:?}",
            other
        ),
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
            println!(
                "demo.restored_cycle_index: {}",
                step.updated_state.cycle_index
            );
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
                import_external_snapshot_as_runtime_state(ExternalRuntimeSnapshot::CodexBootstrap(
                    snapshot,
                ))
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
            let previous =
                muldex_runtime::continuity::export_latest_report_raw(&host, &session_id)?;
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
    match (
        path.parent(),
        path.file_stem().and_then(|stem| stem.to_str()),
    ) {
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

fn daemon_force_takeover(
    path: PathBuf,
    threshold_ms: u64,
) -> Result<(), Box<dyn std::error::Error>> {
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
fn load_client_session_list_view(
    path: PathBuf,
) -> Result<muldex_runtime::client_views::ClientSessionListView, Box<dyn std::error::Error>> {
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
            let previous =
                muldex_runtime::continuity::export_latest_report_raw(&host, &session_id)?;
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
        Some("/config") => match parts.next() {
            Some("llm") | None => InteractiveSlashCommand::ConfigLlm(
                parse_interactive_llm_config_command(parts.collect()),
            ),
            Some(other) => InteractiveSlashCommand::Unknown(format!("/config {other}")),
        },
        Some("/provider") => {
            InteractiveSlashCommand::Provider(parse_interactive_provider_command(parts.collect()))
        }
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
        usage: ShellUsage::default(),
        rate_limit: ShellRateLimit::default(),
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
    let config: MuldexConfig = serde_json::from_str(&raw)?;
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
    let suffix = api_key
        .chars()
        .rev()
        .take(4)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("***{suffix}")
}

fn llm_router_provider(config: &MuldexConfig) -> Option<ProviderConfig> {
    config.providers.get("llm-router").cloned().or_else(|| {
        config.llm_router.as_ref().map(|legacy| ProviderConfig {
            kind: "openai-compatible".to_string(),
            host: Some(legacy.host.clone()),
            port: Some(legacy.port),
            base_url: None,
            api_key: Some(legacy.api_key.clone()),
            api_key_env: None,
            default_model: legacy.default_model.clone(),
        })
    })
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
    if matches!(std::env::var("MULDEX_FORCE_PLAIN_SHELL"), Ok(value) if value == "1") {
        return true;
    }
    !interactive_shell_tty_enabled()
}

fn interactive_shell_tty_enabled() -> bool {
    if matches!(std::env::var("MULDEX_FORCE_TTY_RENDER"), Ok(value) if value == "1") {
        return io::stdout().is_terminal() && io::stdin().is_terminal();
    }
    io::stdout().is_terminal() && io::stdin().is_terminal()
}

fn interactive_shell_line_input_enabled() -> bool {
    interactive_shell_plain_output_enabled() || !io::stdin().is_terminal()
}

fn interactive_shell_exit_notice_lines(session_id: &str) -> [String; 2] {
    [
        "leaving muldex interactive shell".to_string(),
        format!("To continue this session, run muldex resume {session_id}"),
    ]
}

fn emit_interactive_shell_exit_notice(
    tui_session: &mut Option<interactive_tui::TuiTerminalSession>,
    session_id: &str,
) {
    let _ = tui_session.take();
    for line in interactive_shell_exit_notice_lines(session_id) {
        println!("{line}");
    }
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
        InteractiveSlashHint {
            command: "/help",
            summary: "show available commands",
        },
        InteractiveSlashHint {
            command: "/status",
            summary: "show runtime state",
        },
        InteractiveSlashHint {
            command: "/model",
            summary: "show or set active model",
        },
        InteractiveSlashHint {
            command: "/provider",
            summary: "show, list, or switch provider",
        },
        InteractiveSlashHint {
            command: "/approval",
            summary: "show or set approval mode",
        },
        InteractiveSlashHint {
            command: "/compact",
            summary: "request compaction",
        },
        InteractiveSlashHint {
            command: "/sessions",
            summary: "list resumable sessions",
        },
        InteractiveSlashHint {
            command: "/resume",
            summary: "resume active or named session",
        },
        InteractiveSlashHint {
            command: "/new",
            summary: "create a fresh session",
        },
        InteractiveSlashHint {
            command: "/exit",
            summary: "leave interactive shell",
        },
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

fn active_provider_summary(
    config: &MuldexConfig,
    runtime: &RuntimeState,
    shell: &InteractiveShellState,
) -> String {
    match resolve_provider_config(config, None) {
        Ok(provider) => {
            let model = provider
                .default_model
                .clone()
                .unwrap_or_else(|| runtime_model_label(runtime, shell));
            format!("{} / {}", provider.name, model)
        }
        Err(_) => String::new(),
    }
}

fn active_provider_is_configured(config: &MuldexConfig) -> bool {
    resolve_provider_config(config, None).is_ok()
}

fn append_system_messages_to_active_session(
    contents: impl IntoIterator<Item = String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let active_session_id = store.active_session_id.clone();
    if let Some(snapshot) = store
        .sessions
        .iter_mut()
        .find(|snapshot| snapshot.session_id == active_session_id)
    {
        for content in contents {
            snapshot.messages.push(InteractiveMessage {
                role: InteractiveMessageRole::System,
                content,
            });
        }
    }
    save_interactive_shell_store(&store)?;
    Ok(())
}

fn interactive_shell_help_lines() -> Vec<String> {
    vec![
        "/help     show available commands".to_string(),
        "/status   show runtime phase and cycle".to_string(),
        "/config llm show current llm-router config".to_string(),
        "/provider show current provider or switch with /provider use <name>".to_string(),
        "/model    show or set active model".to_string(),
        "/approval show or set approval mode".to_string(),
        "/compact  record a compaction request".to_string(),
        "/sessions list resumable interactive shells".to_string(),
        "/resume   restore active shell or a named session".to_string(),
        "/new      create a fresh interactive shell session".to_string(),
        "/exit     leave interactive shell".to_string(),
    ]
}

fn interactive_shell_status_lines(
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
) -> Vec<String> {
    let config = load_muldex_config().unwrap_or_default();
    let mut lines = vec![
        format!("session.phase: {:?}", driver.state.phase),
        format!("session.cycle_index: {}", driver.state.cycle_index),
        format!("session.objective: {}", driver.state.request.objective),
        format!(
            "session.model: {}",
            runtime_model_label(&driver.state, shell)
        ),
        format!(
            "session.approval_mode: {}",
            approval_policy_label(&driver.state.request.safety.approval_policy)
        ),
        format!(
            "session.requires_explicit_approval_for_next_step: {}",
            driver
                .state
                .request
                .safety
                .requires_explicit_approval_for_next_step
        ),
        format!("session.compact_count: {}", shell.compact_count),
        format!("session.resume_count: {}", shell.resume_count),
        format!(
            "session.pending_post_compaction: {}",
            driver.state.request.post_compaction.pending_post_compaction
        ),
        format!(
            "session.first_post_compaction_turn: {}",
            driver
                .state
                .request
                .post_compaction
                .first_post_compaction_turn
        ),
        format!(
            "session.compaction_window_id: {:?}",
            driver.state.request.post_compaction.compaction_window_id
        ),
    ];

    match llm_router_provider(&config) {
        Some(router) => {
            lines.push(format!("llm_router.host: {:?}", router.host));
            lines.push(format!("llm_router.port: {:?}", router.port));
            lines.push(format!(
                "llm_router.api_key: {}",
                masked_api_key(router.api_key.as_deref().unwrap_or(""))
            ));
            lines.push(format!(
                "llm_router.default_model: {:?}",
                router.default_model
            ));
        }
        None => lines.push("llm_router.configured: false".to_string()),
    }

    lines.push(format!("default_provider: {:?}", config.default_provider));
    if let Some(active_name) = active_provider_name(&config) {
        if let Some(provider) = config.providers.get(&active_name) {
            lines.push(format!("active_provider.kind: {}", provider.kind));
            lines.push(format!("active_provider.base_url: {:?}", provider.base_url));
            lines.push(format!("active_provider.host: {:?}", provider.host));
            lines.push(format!("active_provider.port: {:?}", provider.port));
            lines.push(format!(
                "active_provider.default_model: {:?}",
                provider.default_model
            ));
        }
    }
    if let Some(report) = driver.state.latest_report.as_ref() {
        lines.push(format!("session.last_outcome: {:?}", report.outcome));
        lines.push(format!("session.last_rationale: {}", report.rationale));
    }
    lines
}

fn interactive_shell_emit_line(line: impl AsRef<str>) {
    if interactive_shell_plain_output_enabled() {
        println!("{}", line.as_ref());
    }
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
        buffer.replace_first_line(
            completion
                .current_command()
                .expect("current command present"),
        );
        return true;
    }

    if completion.matches.is_empty() || !completion.seed.starts_with('/') {
        completion.update_from_buffer(buffer);
    } else if completion
        .current_command()
        .is_some_and(|command| command == buffer.first_line())
    {
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
    tui_session: &mut Option<&mut interactive_tui::TuiTerminalSession>,
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    session_id: &str,
    buffer: &InteractivePromptBuffer,
    completion: &InteractiveSlashCompletionState,
    history: &InteractiveHistoryState,
    search: &InteractiveHistorySearchState,
    overlay: &interactive_tui::overlay::OverlayState,
) -> Result<(), Box<dyn std::error::Error>> {
    render_interactive_shell_view(
        tui_session.as_deref_mut(),
        driver,
        shell,
        session_id,
        buffer,
        completion,
        history,
        search,
        overlay,
    )?;
    Ok(())
}

fn handle_interactive_key_event(
    key_event: KeyEvent,
    buffer: &mut InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
    history: &mut InteractiveHistoryState,
    search: &mut InteractiveHistorySearchState,
    overlay: &mut interactive_tui::overlay::OverlayState,
    session_id: &str,
) -> InteractiveKeyAction {
    if !matches!(key_event.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
        return InteractiveKeyAction::Noop;
    }

    let keymap = init_keymap();

    if overlay.visible {
        if let Some(action) = keymap.match_pager_action(&key_event) {
            return match action {
                "scroll_up" => {
                    overlay.scroll_up(1);
                    InteractiveKeyAction::RedrawFrame
                }
                "scroll_down" => {
                    overlay.scroll_down(1);
                    InteractiveKeyAction::RedrawFrame
                }
                "page_up" => {
                    overlay.page_up(10);
                    InteractiveKeyAction::RedrawFrame
                }
                "page_down" => {
                    overlay.page_down(10);
                    InteractiveKeyAction::RedrawFrame
                }
                "jump_top" => {
                    overlay.jump_top();
                    InteractiveKeyAction::RedrawFrame
                }
                "jump_bottom" => {
                    overlay.jump_bottom();
                    InteractiveKeyAction::RedrawFrame
                }
                "close" => {
                    overlay.hide();
                    InteractiveKeyAction::RedrawFrame
                }
                _ => InteractiveKeyAction::Noop,
            };
        }
        if let Some(action) = keymap.match_approval_action(&key_event) {
            match action {
                "approve" | "approve_session" => {
                    overlay.hide();
                    return InteractiveKeyAction::Status;
                }
                "deny" | "cancel" => {
                    overlay.hide();
                    return InteractiveKeyAction::RedrawFrame;
                }
                _ => return InteractiveKeyAction::Noop,
            }
        }
        return InteractiveKeyAction::Noop;
    }

    if !search.is_active() && !completion.visible {
        if let Some(action) = vim_handle_key(key_event, buffer) {
            return action;
        }
    }

    if let Some(action) = keymap.match_app_action(&key_event) {
        match action {
            "clear_terminal" => return InteractiveKeyAction::ClearScreen,
            "open_external_editor" => return InteractiveKeyAction::OpenExternalEditor,
            "copy" => return InteractiveKeyAction::Copy,
            "open_transcript" => {
                let lines = transcript_lines_for_pager(session_id);
                *overlay = interactive_tui::overlay::OverlayState::show_pager(
                    "Transcript History".to_string(),
                    lines,
                );
                return InteractiveKeyAction::RedrawFrame;
            }
            _ => {}
        }
    }

    if let Some(action) = keymap.match_chat_action(&key_event) {
        match action {
            "interrupt_turn" => return InteractiveKeyAction::Exit,
            _ => {}
        }
    }

    if let Some(action) = keymap.match_composer_action(&key_event) {
        match action {
            "submit" => {
                if buffer.first_line().starts_with('/')
                    && completion.current_command().is_some()
                    && completion.current_command() != Some(buffer.first_line())
                {
                    buffer.replace_first_line(
                        completion
                            .current_command()
                            .expect("current command present"),
                    );
                    return InteractiveKeyAction::RedrawFrame;
                }
                let input = parse_interactive_shell_input(&buffer.text);
                completion.reset();
                search.reset();
                buffer.clear();
                history.index_from_end = None;
                history.draft = None;
                return InteractiveKeyAction::Submit(input);
            }
            "queue" => {
                if apply_interactive_slash_completion(buffer, completion) {
                    return InteractiveKeyAction::RedrawFrame;
                }
                return InteractiveKeyAction::Noop;
            }
            "history_search_previous" => {
                completion.reset();
                if search.reverse_search(history, buffer) {
                    return InteractiveKeyAction::RedrawFrame;
                }
                return InteractiveKeyAction::Noop;
            }
            "history_search_next" => {
                return InteractiveKeyAction::RedrawFrame;
            }
            _ => {}
        }
    }

    if let Some(action) = keymap.match_editor_action(&key_event) {
        return match action {
            "insert_newline" => {
                completion.reset();
                search.reset();
                buffer.insert_newline();
                InteractiveKeyAction::RedrawPrompt
            }
            "move_left" => {
                completion.reset();
                search.reset();
                buffer.move_left();
                InteractiveKeyAction::RedrawPrompt
            }
            "move_right" => {
                completion.reset();
                search.reset();
                buffer.move_right();
                InteractiveKeyAction::RedrawPrompt
            }
            "move_up" => {
                if move_interactive_slash_selection(buffer, completion, -1)
                    || history.previous(buffer)
                {
                    if completion.visible {
                        InteractiveKeyAction::RedrawFrame
                    } else {
                        InteractiveKeyAction::RedrawPrompt
                    }
                } else {
                    InteractiveKeyAction::Noop
                }
            }
            "move_down" => {
                if move_interactive_slash_selection(buffer, completion, 1)
                    || history.next(buffer)
                {
                    if completion.visible {
                        InteractiveKeyAction::RedrawFrame
                    } else {
                        InteractiveKeyAction::RedrawPrompt
                    }
                } else {
                    InteractiveKeyAction::Noop
                }
            }
            "move_word_left" => {
                completion.reset();
                search.reset();
                buffer.move_word_left();
                InteractiveKeyAction::RedrawPrompt
            }
            "move_word_right" => {
                completion.reset();
                search.reset();
                buffer.move_word_right();
                InteractiveKeyAction::RedrawPrompt
            }
            "move_line_start" => {
                completion.reset();
                search.reset();
                buffer.move_home();
                InteractiveKeyAction::RedrawPrompt
            }
            "move_line_end" => {
                completion.reset();
                search.reset();
                buffer.move_end();
                InteractiveKeyAction::RedrawPrompt
            }
            "delete_backward" => {
                if search.backspace_query(history, buffer) {
                    completion.reset();
                    return InteractiveKeyAction::RedrawFrame;
                }
                completion.reset();
                search.reset();
                buffer.backspace();
                InteractiveKeyAction::RedrawPrompt
            }
            "delete_backward_word" => {
                completion.reset();
                history.index_from_end = None;
                history.draft = None;
                search.reset();
                buffer.delete_word_left();
                InteractiveKeyAction::RedrawPrompt
            }
            "kill_line_start" => {
                completion.reset();
                search.reset();
                buffer.clear();
                InteractiveKeyAction::RedrawPrompt
            }
            "yank" => return InteractiveKeyAction::Yank,
            _ => InteractiveKeyAction::Noop,
        };
    }

    match key_event {
        KeyEvent {
            code: KeyCode::Char('c' | 'd'),
            modifiers,
            ..
        } if modifiers.contains(KeyModifiers::CONTROL) => InteractiveKeyAction::Exit,
        KeyEvent {
            code: KeyCode::Esc, ..
        } => {
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
        KeyEvent {
            code: KeyCode::Char(ch),
            modifiers,
            ..
        } if !modifiers.contains(KeyModifiers::CONTROL)
            && !modifiers.contains(KeyModifiers::ALT) =>
        {
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
    mut tui_session: Option<&mut interactive_tui::TuiTerminalSession>,
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    session_id: &str,
    buffer: &mut InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
    history: &mut InteractiveHistoryState,
    search: &mut InteractiveHistorySearchState,
    overlay: &mut interactive_tui::overlay::OverlayState,
) -> Result<Option<InteractiveShellInput>, Box<dyn std::error::Error>> {
    if !interactive_tui::win32_input::poll_console_input()? {
        return Ok(None);
    }
    loop {
        match interactive_tui::win32_input::read_console_input()? {
            interactive_tui::win32_input::ConsoleInput::Key(key_event) => {
                match handle_interactive_key_event(
                    key_event,
                    buffer,
                    completion,
                    history,
                    search,
                    overlay,
                    session_id,
                ) {
                    InteractiveKeyAction::Noop => {}
InteractiveKeyAction::RedrawPrompt => {
                        render_interactive_shell_input_frame(
                            &mut tui_session,
                            driver,
                            shell,
                            session_id,
                            buffer,
                            completion,
                            history,
                            search,
                            overlay,
                        )?;
                    }
InteractiveKeyAction::RedrawFrame => {
                        render_interactive_shell_input_frame(
                            &mut tui_session,
                            driver,
                            shell,
                            session_id,
                            buffer,
                            completion,
                            history,
                            search,
                            overlay,
                        )?;
                    }
InteractiveKeyAction::ClearScreen => {
                        if let Some(session) = tui_session.as_deref_mut() {
                            session.clear_scrollback()?;
                        }
                        render_interactive_shell_input_frame(
                            &mut tui_session,
                            driver,
                            shell,
                            session_id,
                            buffer,
                            completion,
                            history,
                            search,
                            overlay,
                        )?;
                    }
InteractiveKeyAction::OpenExternalEditor => {
                        if let Some(session) = tui_session.as_deref_mut() {
                            session.suspend()?;
                        }
                        if let Some(edited) = open_external_editor(&buffer.text) {
                            buffer.text = edited;
                        }
                        if let Some(session) = tui_session.as_deref_mut() {
                            session.resume()?;
                        }
                        render_interactive_shell_input_frame(
                            &mut tui_session,
                            driver,
                            shell,
                            session_id,
                            buffer,
                            completion,
                            history,
                            search,
                            overlay,
                        )?;
                    }
InteractiveKeyAction::Copy => {
                        let _ = copy_to_clipboard(&buffer.text);
                        interactive_tui::notifications::notify(
                            "copied to clipboard",
                            Duration::from_secs(2),
                        );
                        render_interactive_shell_input_frame(
                            &mut tui_session,
                            driver,
                            shell,
                            session_id,
                            buffer,
                            completion,
                            history,
                            search,
                            overlay,
                        )?;
                    }
                    InteractiveKeyAction::Yank => {
                        let _ = copy_to_clipboard(&buffer.text);
                        interactive_tui::notifications::notify(
                            "yanked to clipboard",
                            Duration::from_secs(2),
                        );
                        render_interactive_shell_input_frame(
                            &mut tui_session,
                            driver,
                            shell,
                            session_id,
                            buffer,
                            completion,
                            history,
                            search,
                            overlay,
                        )?;
                    }
                    InteractiveKeyAction::Exit => {
                        return Ok(Some(InteractiveShellInput::Exit));
                    }
                    InteractiveKeyAction::Status => {
                        return Ok(Some(InteractiveShellInput::Status));
                    }
                    InteractiveKeyAction::Submit(input) => {
                        return Ok(Some(input));
                    }
                }
            }
            interactive_tui::win32_input::ConsoleInput::Resize => {
                render_interactive_shell_input_frame(
                    &mut tui_session,
                    driver,
                    shell,
                    session_id,
                    buffer,
                    completion,
                    history,
                    search,
                    overlay,
                )?;
            }
            interactive_tui::win32_input::ConsoleInput::None => {}
        }
    }
}

fn read_interactive_shell_scripted_event(
    mut tui_session: Option<&mut interactive_tui::TuiTerminalSession>,
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    session_id: &str,
    buffer: &mut InteractivePromptBuffer,
    completion: &mut InteractiveSlashCompletionState,
    history: &mut InteractiveHistoryState,
    search: &mut InteractiveHistorySearchState,
    overlay: &mut interactive_tui::overlay::OverlayState,
    scripted: &mut InteractiveScriptedKeysState,
) -> Result<Option<InteractiveShellInput>, Box<dyn std::error::Error>> {
    let Some(key_event) = scripted.events.pop_front() else {
        return Ok(Some(InteractiveShellInput::Exit));
    };

    match handle_interactive_key_event(key_event, buffer, completion, history, search, overlay, session_id) {
        InteractiveKeyAction::Noop => Ok(None),
        InteractiveKeyAction::RedrawPrompt => {
            render_interactive_shell_input_frame(
                &mut tui_session,
                driver,
                shell,
                session_id,
                buffer,
                completion,
                history,
                search,
                overlay,
            )?;
            Ok(None)
        }
        InteractiveKeyAction::RedrawFrame => {
            render_interactive_shell_input_frame(
                &mut tui_session,
                driver,
                shell,
                session_id,
                buffer,
                completion,
                history,
                search,
                overlay,
            )?;
            Ok(None)
        }
        InteractiveKeyAction::ClearScreen => {
            if let Some(session) = tui_session.as_deref_mut() {
                session.clear_scrollback()?;
            }
render_interactive_shell_input_frame(
                            &mut tui_session,
                            driver,
                shell,
                session_id,
                buffer,
                completion,
                history,
                search,
                overlay,
            )?;
            Ok(None)
        }
        InteractiveKeyAction::Exit => Ok(Some(InteractiveShellInput::Exit)),
        InteractiveKeyAction::Status => Ok(Some(InteractiveShellInput::Status)),
        InteractiveKeyAction::OpenExternalEditor => Ok(None),
        InteractiveKeyAction::Copy => Ok(None),
        InteractiveKeyAction::Yank => Ok(None),
        InteractiveKeyAction::Submit(input) => Ok(Some(input)),
    }
}

fn load_interactive_shell_store()
-> Result<Option<InteractiveShellStore>, Box<dyn std::error::Error>> {
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
        .ok_or_else(|| {
            format!(
                "interactive shell active session not found: {}",
                store.active_session_id
            )
            .into()
        })
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

fn remove_last_system_messages(count: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let active_session_id = store.active_session_id.clone();
    if let Some(snapshot) = store
        .sessions
        .iter_mut()
        .find(|snapshot| snapshot.session_id == active_session_id)
    {
        let removed: Vec<_> = snapshot.messages.drain(
            snapshot.messages.len().saturating_sub(count)..
        ).collect();
        drop(removed);
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

/// Replace the trailing assistant message with `content`, or append a new
/// assistant message when the last message is not an assistant. Used to stream
/// the in-progress reply into a single transcript cell (markdown-rendered live)
/// instead of appending a fresh system message per delta.
fn replace_last_assistant_message(
    content: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let active_session_id = store.active_session_id.clone();
    if let Some(snapshot) = store
        .sessions
        .iter_mut()
        .find(|snapshot| snapshot.session_id == active_session_id)
    {
        if let Some(last) = snapshot.messages.last_mut() {
            if matches!(last.role, InteractiveMessageRole::Assistant) {
                last.content = content.to_string();
                save_interactive_shell_store(&store)?;
                return Ok(());
            }
        }
        snapshot
            .messages
            .push(InteractiveMessage {
                role: InteractiveMessageRole::Assistant,
                content: content.to_string(),
            });
    }
    save_interactive_shell_store(&store)?;
    Ok(())
}

fn append_prompt_history_to_active_session(
    entry: String,
) -> Result<(), Box<dyn std::error::Error>> {
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

fn interactive_session_messages_as_provider_messages(
) -> Result<Vec<ProviderTurnMessage>, Box<dyn std::error::Error>> {
    let store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let snapshot = active_interactive_shell_snapshot(&store)?;
    Ok(snapshot
        .messages
        .iter()
        .filter_map(|message| match message.role {
            InteractiveMessageRole::System => Some(ProviderTurnMessage {
                role: ProviderMessageRole::System,
                content: Some(message.content.clone()),
                tool_call_id: None,
                name: None,
                tool_calls: Vec::new(),
            }),
            InteractiveMessageRole::User => Some(ProviderTurnMessage {
                role: ProviderMessageRole::User,
                content: Some(message.content.clone()),
                tool_call_id: None,
                name: None,
                tool_calls: Vec::new(),
            }),
            InteractiveMessageRole::Assistant => Some(ProviderTurnMessage {
                role: ProviderMessageRole::Assistant,
                content: Some(message.content.clone()),
                tool_call_id: None,
                name: None,
                tool_calls: Vec::new(),
            }),
        })
        .collect())
}

#[derive(Default, Clone)]
struct LiveTurnState {
    content: Arc<Mutex<String>>,
    busy: Arc<Mutex<bool>>,
    done: Arc<Mutex<bool>>,
}

impl LiveTurnState {
    fn push_delta(&self, delta: &str) {
        let mut content = self.content.lock().expect("live turn content");
        content.push_str(delta);
        *self.busy.lock().expect("live turn busy") = true;
    }

    fn mark_done(&self) {
        *self.done.lock().expect("live turn done") = true;
        *self.busy.lock().expect("live turn busy") = false;
    }
}

#[derive(Default)]
struct ShellTurnEventListener {
    events: Vec<UiEvent>,
    live: Option<LiveTurnState>,
    tx: Option<mpsc::Sender<UiEvent>>,
}

impl UiEventListener for ShellTurnEventListener {
    fn on_event(&mut self, event: UiEvent) {
        if let UiEvent::AssistantDelta { delta } = &event {
            if let Some(live) = &self.live {
                live.push_delta(delta);
            }
        }
        if matches!(&event, UiEvent::TurnCompleted | UiEvent::TurnFailed { .. }) {
            if let Some(live) = &self.live {
                live.mark_done();
            }
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(event.clone());
        }
        self.events.push(event);
    }
}

struct ShellReadOnlyToolExecutor {
    driver_state: RuntimeState,
}

impl ShellReadOnlyToolExecutor {
    fn from_driver(driver: &RuntimeDriver) -> Self {
        Self {
            driver_state: driver.state.clone(),
        }
    }

    fn from_driver_state(state: RuntimeState) -> Self {
        Self { driver_state: state }
    }

    fn from_driver_ref(state: &RuntimeState) -> Self {
        Self {
            driver_state: state.clone(),
        }
    }

    fn into_driver_state(self) -> RuntimeState {
        self.driver_state
    }
}

impl InteractiveToolExecutor for ShellReadOnlyToolExecutor {
    fn tool_specs(&self) -> Vec<muldex_core::provider::ProviderToolSpec> {
        vec![
            muldex_core::provider::ProviderToolSpec {
                name: "session.status".to_string(),
                description: "Show current session status".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{}}),
            },
            muldex_core::provider::ProviderToolSpec {
                name: "session.list".to_string(),
                description: "List resumable sessions".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{}}),
            },
            muldex_core::provider::ProviderToolSpec {
                name: "runtime.inspect".to_string(),
                description: "Inspect runtime objective and phase".to_string(),
                input_schema: serde_json::json!({"type":"object","properties":{}}),
            },
        ]
    }

    fn execute(
        &mut self,
        call: &muldex_core::provider::ProviderToolCall,
    ) -> Result<String, InteractiveToolError> {
        match call.name.as_str() {
            "session.status" => Ok(serde_json::json!({
                "phase": format!("{:?}", self.driver_state.phase),
                "cycle_index": self.driver_state.cycle_index,
                "objective": self.driver_state.request.objective,
            })
            .to_string()),
            "session.list" => {
                let store = load_interactive_shell_store()
                    .map_err(|error| InteractiveToolError::Failed(error.to_string()))?
                    .unwrap_or_else(interactive_shell_store_with_default_session);
                Ok(serde_json::json!({
                    "active_session_id": store.active_session_id,
                    "sessions": store
                        .sessions
                        .into_iter()
                        .map(|snapshot| serde_json::json!({
                            "session_id": snapshot.session_id,
                            "phase": format!("{:?}", snapshot.runtime.phase),
                            "cycle_index": snapshot.runtime.cycle_index,
                        }))
                        .collect::<Vec<_>>()
                })
                .to_string())
            }
            "runtime.inspect" => Ok(serde_json::json!({
                "thread_id": self.driver_state.request.thread_id,
                "turn_id": self.driver_state.request.turn_id,
                "phase": format!("{:?}", self.driver_state.phase),
                "objective": self.driver_state.request.objective,
            })
            .to_string()),
            other => Err(InteractiveToolError::Unsupported(other.to_string())),
        }
    }
}

struct ShellInteractiveToolExecutor {
    driver_state: RuntimeState,
    pending_approval: Option<PendingApproval>,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    tool_name: String,
    call_id: String,
    input: serde_json::Value,
    summary: String,
    approved: bool,
}

impl ShellInteractiveToolExecutor {
    fn from_driver(driver: &RuntimeDriver) -> Self {
        Self {
            driver_state: driver.state.clone(),
            pending_approval: None,
        }
    }

    fn from_driver_state(state: RuntimeState) -> Self {
        Self {
            driver_state: state,
            pending_approval: None,
        }
    }

    fn into_driver_state(self) -> RuntimeState {
        self.driver_state
    }
}

impl InteractiveToolExecutor for ShellInteractiveToolExecutor {
    fn tool_specs(&self) -> Vec<muldex_core::provider::ProviderToolSpec> {
        let mut specs = ShellReadOnlyToolExecutor::from_driver_ref(&self.driver_state).tool_specs();
        specs.extend(vec![
            muldex_core::provider::ProviderToolSpec {
                name: "file.write".to_string(),
                description: "Write content to a file (requires approval)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File path to write"},
                        "content": {"type": "string", "description": "Content to write"}
                    },
                    "required": ["path", "content"]
                }),
            },
            muldex_core::provider::ProviderToolSpec {
                name: "shell.exec".to_string(),
                description: "Execute a shell command (requires approval)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "command": {"type": "string", "description": "Command to execute"},
                        "args": {"type": "array", "items": {"type": "string"}, "description": "Arguments"}
                    },
                    "required": ["command"]
                }),
            },
        ]);
        specs
    }

    fn execute(
        &mut self,
        call: &muldex_core::provider::ProviderToolCall,
    ) -> Result<String, InteractiveToolError> {
        if let Some(ref approval) = self.pending_approval {
            if call.name != "approval.respond" {
                return Err(InteractiveToolError::Failed(
                    "Must respond to pending approval first".to_string(),
                ));
            }
        }

        match call.name.as_str() {
            "session.status" => Ok(serde_json::json!({
                "phase": format!("{:?}", self.driver_state.phase),
                "cycle_index": self.driver_state.cycle_index,
                "objective": self.driver_state.request.objective,
            })
            .to_string()),
            "session.list" => {
                let store = load_interactive_shell_store()
                    .map_err(|error| InteractiveToolError::Failed(error.to_string()))?
                    .unwrap_or_else(interactive_shell_store_with_default_session);
                Ok(serde_json::json!({
                    "active_session_id": store.active_session_id,
                    "sessions": store
                        .sessions
                        .into_iter()
                        .map(|snapshot| serde_json::json!({
                            "session_id": snapshot.session_id,
                            "phase": format!("{:?}", snapshot.runtime.phase),
                            "cycle_index": snapshot.runtime.cycle_index,
                        }))
                        .collect::<Vec<_>>()
                })
                .to_string())
            }
            "runtime.inspect" => Ok(serde_json::json!({
                "thread_id": self.driver_state.request.thread_id,
                "turn_id": self.driver_state.request.turn_id,
                "phase": format!("{:?}", self.driver_state.phase),
                "objective": self.driver_state.request.objective,
            })
            .to_string()),
            "file.write" => {
                let args: serde_json::Value = serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::json!({}));
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
                if path.is_empty() {
                    return Err(InteractiveToolError::Failed("path is required".to_string()));
                }
                if self.pending_approval.is_none() {
                    self.pending_approval = Some(PendingApproval {
                        tool_name: "file.write".to_string(),
                        call_id: call.id.clone(),
                        input: args.clone(),
                        summary: format!("Write {} bytes to {}", content.len(), path),
                        approved: false,
                    });
                    return Err(InteractiveToolError::ApprovalRequired(format!(
                        "Write {} bytes to {}",
                        content.len(),
                        path
                    )));
                } else if self.pending_approval.as_ref().unwrap().approved {
                    std::fs::write(path, content)
                        .map_err(|e| InteractiveToolError::Failed(e.to_string()))?;
                    self.pending_approval = None;
                    Ok(serde_json::json!({"success": true, "path": path}).to_string())
                } else {
                    self.pending_approval = None;
                    Err(InteractiveToolError::Failed("User denied file write".to_string()))
                }
            }
            "shell.exec" => {
                let args: serde_json::Value = serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::json!({}));
                let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let args_arr = args.get("args").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                if command.is_empty() {
                    return Err(InteractiveToolError::Failed("command is required".to_string()));
                }
                let args_str: Vec<String> = args_arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                let summary = if args_str.is_empty() {
                    command.to_string()
                } else {
                    format!("{} {}", command, args_str.join(" "))
                };
                if self.pending_approval.is_none() {
                    self.pending_approval = Some(PendingApproval {
                        tool_name: "shell.exec".to_string(),
                        call_id: call.id.clone(),
                        input: args.clone(),
                        summary: summary.clone(),
                        approved: false,
                    });
                    return Err(InteractiveToolError::ApprovalRequired(format!(
                        "Execute: {}",
                        summary
                    )));
                } else if self.pending_approval.as_ref().unwrap().approved {
                    let output = std::process::Command::new(command)
                        .args(&args_str)
                        .output()
                        .map_err(|e| InteractiveToolError::Failed(e.to_string()))?;
                    self.pending_approval = None;
                    Ok(serde_json::json!({
                        "stdout": String::from_utf8_lossy(&output.stdout),
                        "stderr": String::from_utf8_lossy(&output.stderr),
                        "status": output.status.code()
                    })
                    .to_string())
                } else {
                    self.pending_approval = None;
                    Err(InteractiveToolError::Failed("User denied shell exec".to_string()))
                }
            }
            other => Err(InteractiveToolError::Unsupported(other.to_string())),
        }
    }
}

impl ShellInteractiveToolExecutor {
    fn exec_file_write(&mut self) -> Result<String, InteractiveToolError> {
        let input = self
            .pending_approval
            .as_ref()
            .map(|a| a.input.clone())
            .unwrap_or_else(|| serde_json::json!({}));
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = input.get("content").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() {
            return Err(InteractiveToolError::Failed("path is required".to_string()));
        }
        std::fs::write(path, content).map_err(|e| InteractiveToolError::Failed(e.to_string()))?;
        self.pending_approval = None;
        Ok(serde_json::json!({ "success": true, "path": path }).to_string())
    }

    fn exec_shell_exec(&mut self) -> Result<String, InteractiveToolError> {
        let input = self
            .pending_approval
            .as_ref()
            .map(|a| a.input.clone())
            .unwrap_or_else(|| serde_json::json!({}));
        let command = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let args_arr = input
            .get("args")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let args_str: Vec<String> = args_arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if command.is_empty() {
            return Err(InteractiveToolError::Failed("command is required".to_string()));
        }
        let output = std::process::Command::new(command)
            .args(&args_str)
            .output()
            .map_err(|e| InteractiveToolError::Failed(e.to_string()))?;
        self.pending_approval = None;
        Ok(serde_json::json!({
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "status": output.status.code(),
        })
        .to_string())
    }

    /// Execute an already-vetted tool call on behalf of the user after the
    /// approval modal resolves. `approve` reflects the user's decision.
    pub fn run_captured(
        &mut self,
        call: &muldex_core::provider::ProviderToolCall,
        approve: bool,
    ) -> Result<String, InteractiveToolError> {
        let input: serde_json::Value =
            serde_json::from_str(&call.arguments_json).unwrap_or(serde_json::json!({}));
        let summary = match call.name.as_str() {
            "file.write" => {
                let path = input.get("path").and_then(|v| v.as_str()).unwrap_or("");
                let len = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                format!("Write {len} bytes to {path}")
            }
            "shell.exec" => {
                let command = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
                let args_arr = input
                    .get("args")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let args_str: Vec<String> = args_arr
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if args_str.is_empty() {
                    command.to_string()
                } else {
                    format!("{command} {}", args_str.join(" "))
                }
            }
            _ => return Err(InteractiveToolError::Unsupported(call.name.clone())),
        };
        if !approve {
            return Err(InteractiveToolError::Failed("User denied approval".to_string()));
        }
        self.pending_approval = Some(PendingApproval {
            tool_name: call.name.clone(),
            call_id: call.id.clone(),
            input,
            summary,
            approved: true,
        });
        match call.name.as_str() {
            "file.write" => self.exec_file_write(),
            "shell.exec" => self.exec_shell_exec(),
            _ => Err(InteractiveToolError::Unsupported(call.name.clone())),
        }
    }
}

fn emit_interactive_ui_event(event: &UiEvent) {
    match event {
        UiEvent::TurnStarted { model, .. } => {
            interactive_shell_emit_line(format!("assistant.model: {model}"));
            interactive_shell_emit_line("assistant.turn_started: true");
        }
        UiEvent::AssistantDelta { delta } => {
            interactive_shell_emit_line(format!("assistant.delta: {delta}"));
        }
        UiEvent::AssistantMessageFinalized { content } => {
            interactive_shell_emit_line(format!("assistant.final: {content}"));
        }
        UiEvent::ToolCallProposed { call } => {
            interactive_shell_emit_line(format!("tool.proposed: {} {}", call.name, call.arguments_json));
        }
        UiEvent::ApprovalRequested { summary } => {
            interactive_shell_emit_line(format!("approval.requested: {summary}"));
        }
        UiEvent::ToolExecutionStarted { tool_name } => {
            interactive_shell_emit_line(format!("tool.started: {tool_name}"));
        }
        UiEvent::ToolExecutionFinished { tool_name, result } => {
            interactive_shell_emit_line(format!("tool.finished: {tool_name} {result}"));
        }
        UiEvent::TurnFailed { error } => {
            interactive_shell_emit_line(format!("assistant.error: {error}"));
        }
        UiEvent::TurnCompleted => {
            interactive_shell_emit_line("assistant.turn_completed: true");
        }
        UiEvent::Usage {
            input_tokens,
            cached_input_tokens,
            output_tokens,
            total_tokens,
        } => {
            interactive_shell_emit_line(format!(
                "assistant.usage: in={input_tokens} cached={cached_input_tokens} out={output_tokens} total={total_tokens}"
            ));
        }
        UiEvent::RateLimit {
            remaining_requests,
            remaining_tokens,
            ..
        } => {
            interactive_shell_emit_line(format!(
                "assistant.rate_limit: remaining_requests={remaining_requests:?} remaining_tokens={remaining_tokens:?}"
            ));
        }
    }
}

fn persist_interactive_ui_events(events: &[UiEvent]) -> Result<(), Box<dyn std::error::Error>> {
    for event in events {
        match event {
            UiEvent::TurnStarted { .. } | UiEvent::TurnCompleted => {}
            UiEvent::AssistantDelta { .. } => {}
            UiEvent::AssistantMessageFinalized { content } => {
                replace_last_assistant_message(content)?;
            }
            UiEvent::ToolCallProposed { call } => {
                append_message_to_active_session(
                    InteractiveMessageRole::System,
                    format!("tool proposed: {} {}", call.name, call.arguments_json),
                )?;
            }
            UiEvent::ApprovalRequested { summary } => {
                append_message_to_active_session(
                    InteractiveMessageRole::System,
                    format!("approval requested: {summary}"),
                )?;
            }
            UiEvent::ToolExecutionStarted { tool_name } => {
                append_message_to_active_session(
                    InteractiveMessageRole::System,
                    format!("tool started: {tool_name}"),
                )?;
            }
            UiEvent::ToolExecutionFinished { tool_name, result } => {
                append_message_to_active_session(
                    InteractiveMessageRole::System,
                    format!("tool finished: {tool_name} {result}"),
                )?;
            }
            UiEvent::TurnFailed { error } => {
                append_message_to_active_session(
                    InteractiveMessageRole::System,
                    format!("assistant error: {error}"),
                )?;
            }
            UiEvent::Usage { .. } | UiEvent::RateLimit { .. } => {}
        }
    }
    Ok(())
}

fn refresh_interactive_shell_session_locals(
    session_id: &mut String,
    shell: &mut InteractiveShellState,
    prompt_buffer: &mut InteractivePromptBuffer,
    completion_state: &mut InteractiveSlashCompletionState,
    history_state: &mut InteractiveHistoryState,
    history_search_state: &mut InteractiveHistorySearchState,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let snapshot = active_interactive_shell_snapshot(&store)?;

    if snapshot.session_id != *session_id {
        *session_id = snapshot.session_id;
        *shell = snapshot.shell.clone();
        *history_state = InteractiveHistoryState::from_entries(snapshot.prompt_history.clone());
        *history_search_state = InteractiveHistorySearchState::default();
        *completion_state = InteractiveSlashCompletionState::default();
        prompt_buffer.clear();
    }

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
                    println!(
                        "session.active: {}",
                        snapshot.session_id == store.active_session_id
                    );
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

fn print_interactive_shell_header(
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    session_id: &str,
) {
    println!("== muldex session ==");
    println!("session.id: {}", session_id);
    println!("session.phase: {:?}", driver.state.phase);
    println!(
        "session.model: {}",
        runtime_model_label(&driver.state, shell)
    );
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

fn transcript_lines_for_pager(session_id: &str) -> Vec<String> {
    let store = match load_interactive_shell_store() {
        Ok(store) => store,
        Err(_) => return vec!["(no transcript available)".to_string()],
    };
    let store = store.unwrap_or_else(interactive_shell_store_with_default_session);
    let Ok(snapshot) = active_interactive_shell_snapshot(&store) else {
        return vec!["(no transcript available)".to_string()];
    };
    let mut lines = vec![format!("session: {session_id}"), String::new()];
    for message in &snapshot.messages {
        let role = match message.role {
            InteractiveMessageRole::System => "SYSTEM",
            InteractiveMessageRole::User => "USER",
            InteractiveMessageRole::Assistant => "ASSISTANT",
        };
        lines.push(format!("[{role}]"));
        for content_line in message.content.split('\n') {
            lines.push(content_line.to_string());
        }
        lines.push(String::new());
    }
    lines
}

fn render_interactive_shell_view(
    tui_session: Option<&mut interactive_tui::TuiTerminalSession>,
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    session_id: &str,
    buffer: &InteractivePromptBuffer,
    completion: &InteractiveSlashCompletionState,
    history: &InteractiveHistoryState,
    search: &InteractiveHistorySearchState,
    overlay: &interactive_tui::overlay::OverlayState,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let snapshot = active_interactive_shell_snapshot(&store)?;
    let view_model = build_interactive_shell_view_model(
        driver,
        shell,
        session_id,
        &snapshot.messages,
        buffer,
        completion,
        history,
        search,
        overlay,
    );

    if interactive_shell_plain_output_enabled() {
        return Ok(());
    }

    if let Some(session) = tui_session {
        set_interactive_cursor_style(session, overlay.visible);
        interactive_tui::draw(session, &view_model)?;
    } else {
        let mut session = interactive_tui::start_terminal_session(false)?;
        set_interactive_cursor_style(&mut session, overlay.visible);
        interactive_tui::draw(&mut session, &view_model)?;
    }
    Ok(())
}

fn set_interactive_cursor_style(
    session: &mut interactive_tui::TuiTerminalSession,
    overlay_visible: bool,
) {
    let vim_normal = vim_state()
        .lock()
        .map(|state| state.enabled && state.normal)
        .unwrap_or(false);
    let result = if overlay_visible {
        session.set_cursor_hidden()
    } else if vim_normal {
        session.set_cursor_style(crossterm::cursor::SetCursorStyle::SteadyBlock)
    } else {
        session.set_cursor_bar()
    };
    if let Err(error) = result {
        let _ = error;
    }
}

fn print_interactive_shell_help() {
    if interactive_shell_plain_output_enabled() {
        for line in interactive_shell_help_lines() {
            println!("{line}");
        }
    }
}

fn print_interactive_shell_status(driver: &RuntimeDriver, shell: &InteractiveShellState) {
    if interactive_shell_plain_output_enabled() {
        for line in interactive_shell_status_lines(driver, shell) {
            println!("{line}");
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
                Err(error) => {
                    format!("llm-router test failed: {host}:{port} unreachable ({error})")
                }
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
        InteractiveLlmConfigCommand::Invalid(reason) => {
            format!("llm-router config error: {reason}")
        }
    };
    Ok(message)
}

fn active_provider_name(config: &MuldexConfig) -> Option<String> {
    muldex_core::provider::active_provider_name(config)
}

fn provider_socket_address(
    provider: &ProviderConfig,
) -> Result<String, Box<dyn std::error::Error>> {
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
                Err(error) => {
                    format!("provider test failed: {name} unreachable at {address} ({error})")
                }
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
            let provider_name = active_provider_name(&config).unwrap_or("not-set".to_string());
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
            interactive_shell_emit_line(format!(
                "session.model: {}",
                runtime_model_label(&driver.state, shell)
            ));
            system_messages.push(format!(
                "/model -> {}",
                runtime_model_label(&driver.state, shell)
            ));
        }
        InteractiveSlashCommand::Model(Some(model)) => {
            shell.model = model.clone();
            ensure_runtime_model_state(&mut driver.state, shell);
            if let Some(continuation) = driver.state.request.codex_continuation.as_mut() {
                continuation.source_model = model.clone();
            }
            interactive_shell_emit_line(format!("session.model_set: {}", model));
            system_messages.push(format!("/model set to {model}"));
        }
        InteractiveSlashCommand::Approval(None) => {
            interactive_shell_emit_line(format!(
                "session.approval_mode: {}",
                approval_policy_label(&driver.state.request.safety.approval_policy)
            ));
            system_messages.push(format!(
                "/approval -> {}",
                approval_policy_label(&driver.state.request.safety.approval_policy)
            ));
        }
        InteractiveSlashCommand::Approval(Some(mode)) => match parse_approval_policy(&mode) {
            Some(policy) => {
                shell.approval_mode = approval_policy_label(&policy).to_string();
                driver.state.request.safety.approval_policy = policy.clone();
                driver
                    .state
                    .request
                    .safety
                    .requires_explicit_approval_for_next_step =
                    matches!(policy, ApprovalPolicyDescriptor::Ask);
                interactive_shell_emit_line(format!(
                    "session.approval_mode_set: {}",
                    shell.approval_mode
                ));
                system_messages.push(format!("/approval set to {}", shell.approval_mode));
            }
            None => {
                interactive_shell_emit_line("session.approval_mode_set: invalid");
                interactive_shell_emit_line("session.approval_mode_error: unsupported_mode");
                system_messages.push(format!("/approval invalid mode: {mode}"));
            }
        },
        InteractiveSlashCommand::Compact => {
            shell.compact_count = shell.compact_count.saturating_add(1);
            driver.state.request.post_compaction.pending_post_compaction = true;
            driver
                .state
                .request
                .post_compaction
                .first_post_compaction_turn = true;
            driver.state.request.post_compaction.compaction_window_id =
                Some(format!("shell-window-{}", shell.compact_count));
            interactive_shell_emit_line("session.compaction_requested: true");
            interactive_shell_emit_line(format!("session.compact_count: {}", shell.compact_count));
            interactive_shell_emit_line(format!(
                "session.compaction_window_id: {:?}",
                driver.state.request.post_compaction.compaction_window_id
            ));
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
                    interactive_shell_emit_line("session.resume_requested: true");
                    interactive_shell_emit_line("session.resumed: true");
                    interactive_shell_emit_line(format!("session.id: {}", store.active_session_id));
                    interactive_shell_emit_line(format!(
                        "session.resume_count: {}",
                        shell.resume_count
                    ));
                    system_messages.push(format!("/resume -> {}", store.active_session_id));
                }
                None => {
                    interactive_shell_emit_line("session.resume_requested: true");
                    interactive_shell_emit_line("session.resumed: false");
                    interactive_shell_emit_line("session.resume_reason: session_not_found");
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
            interactive_shell_emit_line("session.new: true");
            interactive_shell_emit_line(format!("session.id: {}", snapshot.session_id));
            store.sessions.push(snapshot);
        }
        InteractiveSlashCommand::ConfigLlm(command) => {
            let message = handle_interactive_llm_config_command(command)?;
            interactive_shell_emit_line(&message);
            system_messages.push(message);
        }
        InteractiveSlashCommand::Provider(command) => {
            let message = handle_interactive_provider_command(command)?;
            interactive_shell_emit_line(&message);
            system_messages.push(message);
        }
        InteractiveSlashCommand::Unknown(command) => {
            interactive_shell_emit_line(format!("slash command not implemented yet: {command}"));
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
    shell: &mut InteractiveShellState,
    prompt: String,
    tui_session: &mut Option<interactive_tui::TuiTerminalSession>,
    session_id: &str,
    prompt_buffer: &InteractivePromptBuffer,
    completion_state: &InteractiveSlashCompletionState,
    history_state: &InteractiveHistoryState,
    history_search_state: &InteractiveHistorySearchState,
    overlay: &mut interactive_tui::overlay::OverlayState,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_muldex_config()?;
    let resolved_provider = resolve_provider_config(&config, None)?;
    let model = resolved_provider
        .default_model
        .clone()
        .unwrap_or_else(|| runtime_model_label(&driver.state, shell));

    driver.state.request.objective = prompt.clone();
    driver.state.request.continue_reason = ContinueReason::ManualUserRequest;
    append_message_to_active_session(InteractiveMessageRole::User, prompt.clone())?;
    append_prompt_history_to_active_session(prompt.clone())?;

    let prior_messages = interactive_session_messages_as_provider_messages()?;
    let use_tui = tui_session.is_some() && !interactive_shell_plain_output_enabled();
    let approval_mode = std::env::var("MULDEX_APPROVAL_MODE").as_deref() != Ok("off");

    if use_tui {
        let (tx, rx) = mpsc::channel::<UiEvent>();
        let driver_state = driver.state.clone();
        let thread_pc = resolved_provider.clone();
        let thread_model = model.clone();
        let thread_prompt = prompt.clone();
        let thread_messages = prior_messages.clone();

        append_system_messages_to_active_session(["Thinking...".to_string()])?;
        redraw_tui(tui_session, driver, shell, session_id, prompt_buffer, completion_state, history_state, history_search_state, overlay)?;

        let join = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
            let provider = ResponsesProvider::default();
            let mut thread_tools: Box<dyn InteractiveToolExecutor> = if approval_mode {
            Box::new(ShellInteractiveToolExecutor::from_driver_state(driver_state.clone()))
        } else {
            Box::new(ShellReadOnlyToolExecutor::from_driver_state(driver_state.clone()))
        };
            let mut thread_listener = ShellTurnEventListener {
                events: Vec::new(),
                live: None,
                tx: Some(tx),
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.block_on(execute_interactive_turn(
                    &provider,
                    &thread_pc,
                    TurnExecutionRequest {
                        prompt: thread_prompt,
                        model: thread_model,
                        prior_messages: thread_messages,
                        enable_tools: true,
                    },
                    thread_tools.as_mut(),
                    &mut thread_listener,
                ))
            }));
            match result {
                Ok(Ok(execution)) => Ok((execution, thread_listener.events)),
                Ok(Err(error)) => Err(format!("turn execution failed: {error}")),
                Err(_) => Err("turn thread panicked; interface preserved".to_string()),
            }
        });

        let mut accumulated = String::new();
        let mut last_anim = std::time::Instant::now();
        loop {
            let received_delta = match rx.try_recv() {
                Ok(UiEvent::AssistantDelta { delta }) => {
                    accumulated.push_str(&delta);
                    let _ = replace_last_assistant_message(&accumulated);
                    true
                }
                Ok(UiEvent::Usage {
                    input_tokens,
                    cached_input_tokens,
                    output_tokens,
                    total_tokens,
                }) => {
                    shell.usage = ShellUsage {
                        input_tokens,
                        cached_input_tokens,
                        output_tokens,
                        total_tokens,
                    };
                    true
                }
                Ok(UiEvent::RateLimit {
                    limit_requests,
                    remaining_requests,
                    limit_tokens,
                    remaining_tokens,
                    reset_after_seconds,
                }) => {
                    shell.rate_limit = ShellRateLimit {
                        limit_requests,
                        remaining_requests,
                        limit_tokens,
                        remaining_tokens,
                        reset_after_seconds,
                    };
                    true
                }
                Ok(_) => false,
                Err(mpsc::TryRecvError::Empty) => false,
                Err(mpsc::TryRecvError::Disconnected) => break,
            };
            // animate the status spinner between deltas
            if received_delta || last_anim.elapsed() >= Duration::from_millis(150) {
                let _ = redraw_tui(
                    tui_session,
                    driver,
                    shell,
                    session_id,
                    prompt_buffer,
                    completion_state,
                    history_state,
                    history_search_state,
                    overlay,
                );
                last_anim = std::time::Instant::now();
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        let turn_result: Result<
            (
                muldex_runtime::interactive_turn::TurnExecutionResult,
                Vec<muldex_runtime::ui_events::UiEvent>,
            ),
            String,
        > = match join.join() {
            Ok(inner) => inner,
            Err(_) => Err("turn thread panicked; interface preserved".to_string()),
        };
        match turn_result {
            Ok((execution, events)) => {
                remove_last_system_messages(1)?;
                if execution.state.status
                    == muldex_runtime::interactive_turn::InteractiveTurnStatus::AwaitingApproval
                    && approval_mode
                {
                    handle_awaiting_approval(
                        driver,
                        shell,
                        session_id,
                        &prompt,
                        tui_session,
                        prompt_buffer,
                        completion_state,
                        history_state,
                        history_search_state,
                        overlay,
                        execution,
                        events,
                        &resolved_provider,
                        &model,
                    )?;
                } else {
                    finalize_interactive_turn(
                        driver,
                        shell,
                        session_id,
                        &prompt,
                        tui_session,
                        prompt_buffer,
                        completion_state,
                        history_state,
                        history_search_state,
                        overlay,
                        &execution,
                        &events,
                    )?;
                }
            }
            Err(error) => {
                let error_msg = format!("assistant error: {error}");
                interactive_shell_emit_line(&error_msg);
                append_message_to_active_session(InteractiveMessageRole::System, error_msg)?;
                save_active_interactive_shell_snapshot(shell, &driver.state)?;
                interactive_tui::notifications::notify(
                    format!("✗ {error}"),
                    Duration::from_secs(6),
                );
                let _ = redraw_tui(
                    tui_session,
                    driver,
                    shell,
                    session_id,
                    prompt_buffer,
                    completion_state,
                    history_state,
                    history_search_state,
                    overlay,
                );
            }
        }
    } else {
        let mut listener = ShellTurnEventListener { events: Vec::new(), live: None, tx: None };
        let mut tools = ShellReadOnlyToolExecutor::from_driver(driver);

        let runtime = tokio::runtime::Runtime::new()?;
        let execution = runtime.block_on(execute_interactive_turn(
            &ResponsesProvider::default(),
            &resolved_provider,
            TurnExecutionRequest {
                prompt: prompt.clone(),
                model,
                prior_messages,
                enable_tools: true,
            },
            &mut tools,
            &mut listener,
        ))?;

        for event in &listener.events {
            emit_interactive_ui_event(event);
        }
        persist_interactive_ui_events(&listener.events)?;
        driver.state = tools.into_driver_state();

        let rationale = if execution.assistant.content.trim().is_empty() {
            format!("interactive prompt completed: {prompt}")
        } else {
            execution.assistant.content.clone()
        };

        let result = driver.advance(ContinueDecision { allow_continue: true, mode: ContinueMode::NextTurn, rationale: rationale.clone(), next_action: Some("continue interactive session".to_string()), pause_for_approval: false, consume_interrupts_now: false, may_continue_other_work: true, suppress_duplicate_injection: false, downgrade_trigger_turn: false, request_compaction: false, request_handoff_summary: false, request_checkpoint: false, enter_self_correction: false });
        interactive_shell_emit_line(format!("assistant.phase: {:?}", result.updated_state.phase));
        interactive_shell_emit_line(format!("assistant.cycle_index: {}", result.updated_state.cycle_index));
        interactive_shell_emit_line(format!("assistant.outcome: {:?}", result.report.outcome));
        interactive_shell_emit_line(format!("assistant.summary: {}", result.report.rationale));
        save_active_interactive_shell_snapshot(shell, &driver.state)?;
    }
    Ok(())
}

fn next_turn_decision(rationale: String) -> ContinueDecision {
    ContinueDecision {
        allow_continue: true,
        mode: ContinueMode::NextTurn,
        rationale,
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
    }
}

fn finalize_interactive_turn(
    driver: &mut RuntimeDriver,
    shell: &mut InteractiveShellState,
    session_id: &str,
    prompt: &str,
    tui_session: &mut Option<interactive_tui::TuiTerminalSession>,
    prompt_buffer: &InteractivePromptBuffer,
    completion_state: &InteractiveSlashCompletionState,
    history_state: &InteractiveHistoryState,
    history_search_state: &InteractiveHistorySearchState,
    overlay: &interactive_tui::overlay::OverlayState,
    execution: &muldex_runtime::interactive_turn::TurnExecutionResult,
    events: &[muldex_runtime::ui_events::UiEvent],
) -> Result<(), Box<dyn std::error::Error>> {
    for event in events {
        emit_interactive_ui_event(event);
    }
    for event in events {
        match event {
            UiEvent::Usage {
                input_tokens,
                cached_input_tokens,
                output_tokens,
                total_tokens,
            } => {
                shell.usage = ShellUsage {
                    input_tokens: *input_tokens,
                    cached_input_tokens: *cached_input_tokens,
                    output_tokens: *output_tokens,
                    total_tokens: *total_tokens,
                };
            }
            UiEvent::RateLimit {
                limit_requests,
                remaining_requests,
                limit_tokens,
                remaining_tokens,
                reset_after_seconds,
            } => {
                shell.rate_limit = ShellRateLimit {
                    limit_requests: *limit_requests,
                    remaining_requests: *remaining_requests,
                    limit_tokens: *limit_tokens,
                    remaining_tokens: *remaining_tokens,
                    reset_after_seconds: *reset_after_seconds,
                };
            }
            _ => {}
        }
    }
    persist_interactive_ui_events(events)?;

    let rationale = if execution.assistant.content.trim().is_empty() {
        format!("interactive prompt completed: {prompt}")
    } else {
        execution.assistant.content.clone()
    };

    let result = driver.advance(next_turn_decision(rationale.clone()));
    interactive_shell_emit_line(format!("assistant.phase: {:?}", result.updated_state.phase));
    interactive_shell_emit_line(format!("assistant.cycle_index: {}", result.updated_state.cycle_index));
    interactive_shell_emit_line(format!("assistant.outcome: {:?}", result.report.outcome));
    interactive_shell_emit_line(format!("assistant.summary: {}", result.report.rationale));
    save_active_interactive_shell_snapshot(shell, &driver.state)?;
    interactive_tui::notifications::notify(
        "✓ turn complete — press ? for commands",
        Duration::from_secs(4),
    );
    let _ = redraw_tui(
        tui_session,
        driver,
        shell,
        session_id,
        prompt_buffer,
        completion_state,
        history_state,
        history_search_state,
        overlay,
    );
    Ok(())
}

/// Block until the user resolves the approval modal, returning `Some(true)` for
/// approve, `Some(false)` for deny, or `None` for cancel (Ctrl+C). Auto-denies
/// after a long timeout so headless/automated runs cannot hang forever.
fn wait_approval_decision() -> Option<bool> {
    use crossterm::event::{KeyCode, KeyModifiers};
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        if std::time::Instant::now() >= deadline {
            return Some(false);
        }
        if interactive_tui::win32_input::poll_console_input().unwrap_or(false) {
            match interactive_tui::win32_input::read_console_input() {
                Ok(interactive_tui::win32_input::ConsoleInput::Key(k)) => match k.code {
                    KeyCode::Char('a') | KeyCode::Enter => return Some(true),
                    KeyCode::Char('d') | KeyCode::Esc => return Some(false),
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        return None
                    }
                    _ => {}
                },
                _ => {}
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }
}

fn spawn_interactive_turn(
    driver_state: RuntimeState,
    provider_config: muldex_core::provider::ResolvedProviderConfig,
    model: String,
    prompt: String,
    prior_messages: Vec<muldex_core::provider::ProviderTurnMessage>,
    enable_tools: bool,
    approval_mode: bool,
) -> Result<
    (
        muldex_runtime::interactive_turn::TurnExecutionResult,
        Vec<muldex_runtime::ui_events::UiEvent>,
    ),
    String,
> {
    let (tx, rx) = mpsc::channel::<muldex_runtime::ui_events::UiEvent>();
    let join = std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        let provider = ResponsesProvider::default();
        let mut tools: Box<dyn InteractiveToolExecutor> = if approval_mode {
            Box::new(ShellInteractiveToolExecutor::from_driver_state(driver_state.clone()))
        } else {
            Box::new(ShellReadOnlyToolExecutor::from_driver_state(driver_state.clone()))
        };
        let mut listener = ShellTurnEventListener {
            events: Vec::new(),
            live: None,
            tx: Some(tx),
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.block_on(execute_interactive_turn(
                &provider,
                &provider_config,
                TurnExecutionRequest {
                    prompt,
                    model,
                    prior_messages,
                    enable_tools,
                },
                tools.as_mut(),
                &mut listener,
            ))
        }));
        match result {
            Ok(Ok(execution)) => Ok((execution, listener.events)),
            Ok(Err(error)) => Err(format!("turn execution failed: {error}")),
            Err(_) => Err("turn thread panicked; interface preserved".to_string()),
        }
    });
    match join.join() {
        Ok(inner) => inner,
        Err(_) => Err("turn thread panicked; interface preserved".to_string()),
    }
}

fn handle_awaiting_approval(
    driver: &mut RuntimeDriver,
    shell: &mut InteractiveShellState,
    session_id: &str,
    prompt: &str,
    tui_session: &mut Option<interactive_tui::TuiTerminalSession>,
    prompt_buffer: &InteractivePromptBuffer,
    completion_state: &InteractiveSlashCompletionState,
    history_state: &InteractiveHistoryState,
    history_search_state: &InteractiveHistorySearchState,
    overlay: &mut interactive_tui::overlay::OverlayState,
    execution: muldex_runtime::interactive_turn::TurnExecutionResult,
    events: Vec<muldex_runtime::ui_events::UiEvent>,
    resolved_provider: &muldex_core::provider::ResolvedProviderConfig,
    model: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let summary = events
        .iter()
        .find_map(|e| {
            if let muldex_runtime::ui_events::UiEvent::ApprovalRequested { summary } = e {
                Some(summary.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "A tool call requires your approval".to_string());

    *overlay = interactive_tui::overlay::OverlayState::show_approval(
        "Confirm action".to_string(),
        summary.clone(),
    );
    let _ = redraw_tui(
        tui_session,
        driver,
        shell,
        session_id,
        prompt_buffer,
        completion_state,
        history_state,
        history_search_state,
        &*overlay,
    );

    let decision = wait_approval_decision();

    *overlay = interactive_tui::overlay::OverlayState::default();
    let _ = redraw_tui(
        tui_session,
        driver,
        shell,
        session_id,
        prompt_buffer,
        completion_state,
        history_state,
        history_search_state,
        &*overlay,
    );

    let mut new_messages = execution.final_messages.clone();
    if let Some(call) = execution
        .assistant
        .tool_calls
        .iter()
        .find(|c| c.name == "file.write" || c.name == "shell.exec")
    {
        let mut exec = ShellInteractiveToolExecutor::from_driver_state(driver.state.clone());
        let result = match decision {
            Some(true) => exec.run_captured(call, true),
            Some(false) => exec.run_captured(call, false),
            None => Err(InteractiveToolError::Failed("approval cancelled".to_string())),
        };
        let message = result.unwrap_or_else(|e| format!("error: {e}"));
        muldex_runtime::react_loop::append_tool_result_message(
            &mut new_messages,
            &call.name,
            &call.id,
            message,
        );
    }

    let (rerun_execution, rerun_events) = spawn_interactive_turn(
        driver.state.clone(),
        resolved_provider.clone(),
        model.to_string(),
        String::new(),
        new_messages,
        false,
        false,
    )
    .map_err(|e| e)?;

    let combined: Vec<muldex_runtime::ui_events::UiEvent> = events
        .into_iter()
        .chain(rerun_events.into_iter())
        .collect();
    finalize_interactive_turn(
        driver,
        shell,
        session_id,
        prompt,
        tui_session,
        prompt_buffer,
        completion_state,
        history_state,
        history_search_state,
        &*overlay,
        &rerun_execution,
        &combined,
    )?;
    Ok(())
}

fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "clip"])
            .stdin(std::process::Stdio::piped())
            .spawn()?
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())?;
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())?;
        return Ok(());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        // try wl-copy (Wayland), then xclip (X11)
        for cmd in ["wl-copy", "xclip", "xsel"] {
            if std::process::Command::new(cmd)
                .stdin(std::process::Stdio::piped())
                .spawn()
                .is_ok()
            {
                if let Some(mut stdin) = std::process::Command::new(cmd)
                    .stdin(std::process::Stdio::piped())
                    .spawn()?
                    .stdin
                    .take()
                {
                    stdin.write_all(text.as_bytes())?;
                    return Ok(());
                }
            }
        }
        return Err("no clipboard tool found (wl-copy, xclip, or xsel)".into());
    }
    Ok(())
}

fn open_external_editor(initial: &str) -> Option<String> {
    let editor = std::env::var("VISUAL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("EDITOR").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| {
            if cfg!(windows) {
                "notepad".to_string()
            } else {
                "vi".to_string()
            }
        });
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi").to_string();
    let args: Vec<String> = parts.map(|s| s.to_string()).collect();

    let dir = std::env::temp_dir();
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = dir.join(format!("muldex-composer-{stamp}.md"));
    if std::fs::write(&path, initial).is_err() {
        return None;
    }

    let mut command = std::process::Command::new(&program);
    for arg in &args {
        command.arg(arg);
    }
    command.arg(&path);
    let status = command.status();

    let content = std::fs::read_to_string(&path).ok();
    let _ = std::fs::remove_file(&path);

    match status {
        Ok(_) => content,
        Err(error) => {
            interactive_shell_emit_line(&format!("editor '{program}' failed to launch: {error}"));
            None
        }
    }
}

fn redraw_tui(
    tui_session: &mut Option<interactive_tui::TuiTerminalSession>,
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    session_id: &str,
    prompt_buffer: &InteractivePromptBuffer,
    completion_state: &InteractiveSlashCompletionState,
    history_state: &InteractiveHistoryState,
    history_search_state: &InteractiveHistorySearchState,
    overlay: &interactive_tui::overlay::OverlayState,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let snapshot = active_interactive_shell_snapshot(&store)?;
    if let Some(session) = tui_session {
        interactive_tui::draw(session, &build_interactive_shell_view_model(
            driver, shell, session_id, &snapshot.messages, prompt_buffer, completion_state, history_state, history_search_state, overlay,
        ))?;
    }
    Ok(())
}

fn build_interactive_shell_view_model(
    driver: &RuntimeDriver,
    shell: &InteractiveShellState,
    session_id: &str,
    messages: &[InteractiveMessage],
    buffer: &InteractivePromptBuffer,
    completion: &InteractiveSlashCompletionState,
    history: &InteractiveHistoryState,
    search: &InteractiveHistorySearchState,
    overlay: &interactive_tui::overlay::OverlayState,
) -> interactive_tui::ShellViewModel {
    let config = load_muldex_config().unwrap_or_default();
    let provider_summary = active_provider_summary(&config, &driver.state, shell);
    let slash_hints = filtered_interactive_slash_hints(buffer);
    interactive_tui::view_model::build_shell_view_model(
        interactive_tui::view_model::ShellViewModelInput {
            session_id,
            runtime: &driver.state,
            shell,
            messages,
            provider_summary: &provider_summary,
            prompt_buffer: buffer,
            completion,
            history,
            search,
            slash_hints: &slash_hints,
            overlay,
        },
    )
}

fn run_interactive_shell(initial_prompt: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    let store = load_interactive_shell_store()?
        .unwrap_or_else(interactive_shell_store_with_default_session);
    let snapshot = active_interactive_shell_snapshot(&store)?;
    let mut session_id = snapshot.session_id.clone();
    let messages = snapshot.messages.clone();
    let prompt_history = snapshot.prompt_history.clone();
    let mut driver = RuntimeDriver::new(snapshot.runtime);
    let mut shell = snapshot.shell;
    let mut prompt_buffer = InteractivePromptBuffer::default();
    let mut completion_state = InteractiveSlashCompletionState::default();
    let mut history_state = InteractiveHistoryState::from_entries(prompt_history);
    let mut history_search_state = InteractiveHistorySearchState::default();
    let mut overlay_state = interactive_tui::overlay::OverlayState::default();
    let mut scripted_keys_state = interactive_scripted_keys_state();
    let stdin = io::stdin();
    let use_line_input = scripted_keys_state.is_none() && interactive_shell_line_input_enabled();
    let mut tui_session = if scripted_keys_state.is_none() && !use_line_input {
        Some(interactive_tui::start_terminal_session(true)?)
    } else {
        None
    };
    let _config_watcher = if tui_session.is_some() {
        Some(start_config_watcher())
    } else {
        None
    };
    if tui_session.is_some() {
        let theme_mode = std::env::var("MULDEX_THEME").unwrap_or_else(|_| "auto".to_string());
        match theme_mode.to_ascii_lowercase().as_str() {
            "light" => interactive_tui::terminal_palette::set_terminal_bg((245, 245, 245)),
            "dark" => interactive_tui::terminal_palette::set_terminal_bg((24, 24, 27)),
            _ => {
                if std::env::var("MULDEX_PROBE_BG").is_ok() {
                    if let Some(bg) = interactive_tui::terminal_palette::probe_background_color() {
                        interactive_tui::terminal_palette::set_terminal_bg(bg);
                    }
                }
            }
        }
    }
    let provider_not_configured = !active_provider_is_configured(&load_muldex_config()?);
    print_interactive_shell_banner();
    if interactive_shell_plain_output_enabled() {
        print_interactive_shell_header(&driver, &shell, &session_id);
        print_interactive_message_log(&messages);
        if provider_not_configured {
            println!(
                "llm-router not configured; use /config llm host <ip>, /config llm port <port>, /config llm api-key <key>"
            );
        }
    } else if provider_not_configured {
        append_system_messages_to_active_session(["llm-router not configured; use /config llm host <ip>, /config llm port <port>, /config llm api-key <key>".to_string()])?;
    }
    render_interactive_shell_view(
        tui_session.as_mut(),
        &driver,
        &shell,
        &session_id,
        &prompt_buffer,
        &completion_state,
        &history_state,
                &history_search_state,
                &overlay_state,
            )?;
    save_active_interactive_shell_snapshot(&shell, &driver.state)?;

    if let Some(prompt) = initial_prompt {
                handle_interactive_prompt(&mut driver, &mut shell, prompt, &mut tui_session, &session_id, &prompt_buffer, &completion_state, &history_state, &history_search_state, &mut overlay_state)?;
        render_interactive_shell_view(
            tui_session.as_mut(),
            &driver,
            &shell,
            &session_id,
            &prompt_buffer,
            &completion_state,
            &history_state,
            &history_search_state,
            &overlay_state,
        )?;
    }

    loop {
        let input = if let Some(scripted) = scripted_keys_state.as_mut() {
                read_interactive_shell_scripted_event(
                    tui_session.as_mut(),
                    &driver,
                    &shell,
                    &session_id,
                    &mut prompt_buffer,
                    &mut completion_state,
                    &mut history_state,
                    &mut history_search_state,
                    &mut overlay_state,
                    scripted,
                )?
        } else if use_line_input {
            print!("> ");
            io::stdout().flush()?;

            let mut line = String::new();
            let bytes = stdin.read_line(&mut line)?;
            if bytes == 0 {
                emit_interactive_shell_exit_notice(&mut tui_session, &session_id);
                break;
            }
            Some(parse_interactive_shell_input(&line))
        } else {
                read_interactive_shell_input_event(
                    tui_session.as_mut(),
                    &driver,
                    &shell,
                    &session_id,
                    &mut prompt_buffer,
                    &mut completion_state,
                    &mut history_state,
                    &mut history_search_state,
                    &mut overlay_state,
                )?
        };

        let Some(input) = input else {
            continue;
        };

        match input {
            InteractiveShellInput::Empty => {}
            InteractiveShellInput::Exit => {
                emit_interactive_shell_exit_notice(&mut tui_session, &session_id);
                break;
            }
            InteractiveShellInput::Help => {
                print_interactive_shell_help();
                if !interactive_shell_plain_output_enabled() {
                    append_system_messages_to_active_session(interactive_shell_help_lines())?;
                }
                render_interactive_shell_view(
                    tui_session.as_mut(),
                    &driver,
                    &shell,
                    &session_id,
                    &prompt_buffer,
                    &completion_state,
                    &history_state,
        &history_search_state,
        &overlay_state,
    )?;
            }
            InteractiveShellInput::Status => {
                print_interactive_shell_status(&driver, &shell);
                if !interactive_shell_plain_output_enabled() {
                    append_system_messages_to_active_session(interactive_shell_status_lines(
                        &driver, &shell,
                    ))?;
                }
                render_interactive_shell_view(
                    tui_session.as_mut(),
                    &driver,
                    &shell,
                    &session_id,
                    &prompt_buffer,
                    &completion_state,
                    &history_state,
        &history_search_state,
        &overlay_state,
    )?;
            }
            InteractiveShellInput::SlashCommand(command) => {
                handle_interactive_slash_command(&mut driver, &mut shell, command)?;
                refresh_interactive_shell_session_locals(
                    &mut session_id,
                    &mut shell,
                    &mut prompt_buffer,
                    &mut completion_state,
                    &mut history_state,
                    &mut history_search_state,
                )?;
                render_interactive_shell_view(
                    tui_session.as_mut(),
                    &driver,
                    &shell,
                    &session_id,
                    &prompt_buffer,
                    &completion_state,
                    &history_state,
        &history_search_state,
        &overlay_state,
    )?;
            }
            InteractiveShellInput::Prompt(prompt) => {
                history_state.record_submission(&prompt);
        handle_interactive_prompt(&mut driver, &mut shell, prompt, &mut tui_session, &session_id, &prompt_buffer, &completion_state, &history_state, &history_search_state, &mut overlay_state)?;
                render_interactive_shell_view(
                    tui_session.as_mut(),
                    &driver,
                    &shell,
                    &session_id,
                    &prompt_buffer,
                    &completion_state,
                    &history_state,
        &history_search_state,
        &overlay_state,
    )?;
            }
        }

        if !use_line_input {
            let mut tui_session_ref = tui_session.as_mut();
            render_interactive_shell_input_frame(
                &mut tui_session_ref,
                &driver,
                &shell,
                &session_id,
                &prompt_buffer,
                &completion_state,
                &history_state,
                &history_search_state,
                &overlay_state,
            )?;
        }
    }

    if let Some(session) = tui_session.as_mut() {
        let _ = session.reset_cursor();
    }

Ok(())
}

mod config {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use crossterm::event::{KeyCode, KeyModifiers};
    use toml;

    use crate::interactive_tui::keymap::{AppKeymap, ChatKeymap, ComposerKeymap, EditorKeymap, ListKeymap, PagerKeymap, ApprovalKeymap, RuntimeKeymap, KeyBinding, plain, ctrl, alt, shift};

    #[derive(Debug, Clone, Default)]
    pub(crate) struct MuldexConfig {
        pub(crate) keymap: Option<KeymapConfig>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct KeymapConfig {
        pub(crate) app: Option<AppKeymapOverride>,
        pub(crate) chat: Option<ChatKeymapOverride>,
        pub(crate) composer: Option<ComposerKeymapOverride>,
        pub(crate) editor: Option<EditorKeymapOverride>,
        pub(crate) pager: Option<PagerKeymapOverride>,
        pub(crate) approval: Option<ApprovalKeymapOverride>,
        pub(crate) list: Option<ListKeymapOverride>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct AppKeymapOverride {
        pub(crate) open_transcript: Option<Vec<KeyBindingDef>>,
        pub(crate) open_external_editor: Option<Vec<KeyBindingDef>>,
        pub(crate) copy: Option<Vec<KeyBindingDef>>,
        pub(crate) clear_terminal: Option<Vec<KeyBindingDef>>,
        pub(crate) toggle_vim_mode: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct ChatKeymapOverride {
        pub(crate) interrupt_turn: Option<Vec<KeyBindingDef>>,
        pub(crate) decrease_reasoning_effort: Option<Vec<KeyBindingDef>>,
        pub(crate) increase_reasoning_effort: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct ComposerKeymapOverride {
        pub(crate) submit: Option<Vec<KeyBindingDef>>,
        pub(crate) queue: Option<Vec<KeyBindingDef>>,
        pub(crate) toggle_shortcuts: Option<Vec<KeyBindingDef>>,
        pub(crate) history_search_previous: Option<Vec<KeyBindingDef>>,
        pub(crate) history_search_next: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct EditorKeymapOverride {
        pub(crate) insert_newline: Option<Vec<KeyBindingDef>>,
        pub(crate) move_left: Option<Vec<KeyBindingDef>>,
        pub(crate) move_right: Option<Vec<KeyBindingDef>>,
        pub(crate) move_up: Option<Vec<KeyBindingDef>>,
        pub(crate) move_down: Option<Vec<KeyBindingDef>>,
        pub(crate) move_word_left: Option<Vec<KeyBindingDef>>,
        pub(crate) move_word_right: Option<Vec<KeyBindingDef>>,
        pub(crate) move_line_start: Option<Vec<KeyBindingDef>>,
        pub(crate) move_line_end: Option<Vec<KeyBindingDef>>,
        pub(crate) delete_backward: Option<Vec<KeyBindingDef>>,
        pub(crate) delete_forward: Option<Vec<KeyBindingDef>>,
        pub(crate) delete_backward_word: Option<Vec<KeyBindingDef>>,
        pub(crate) delete_forward_word: Option<Vec<KeyBindingDef>>,
        pub(crate) kill_line_start: Option<Vec<KeyBindingDef>>,
        pub(crate) kill_whole_line: Option<Vec<KeyBindingDef>>,
        pub(crate) kill_line_end: Option<Vec<KeyBindingDef>>,
        pub(crate) yank: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct PagerKeymapOverride {
        pub(crate) scroll_up: Option<Vec<KeyBindingDef>>,
        pub(crate) scroll_down: Option<Vec<KeyBindingDef>>,
        pub(crate) page_up: Option<Vec<KeyBindingDef>>,
        pub(crate) page_down: Option<Vec<KeyBindingDef>>,
        pub(crate) jump_top: Option<Vec<KeyBindingDef>>,
        pub(crate) jump_bottom: Option<Vec<KeyBindingDef>>,
        pub(crate) close: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct ApprovalKeymapOverride {
        pub(crate) approve: Option<Vec<KeyBindingDef>>,
        pub(crate) approve_session: Option<Vec<KeyBindingDef>>,
        pub(crate) deny: Option<Vec<KeyBindingDef>>,
        pub(crate) cancel: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, Clone, Default)]
    pub(crate) struct ListKeymapOverride {
        pub(crate) move_up: Option<Vec<KeyBindingDef>>,
        pub(crate) move_down: Option<Vec<KeyBindingDef>>,
        pub(crate) accept: Option<Vec<KeyBindingDef>>,
        pub(crate) cancel: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, Clone, serde::Deserialize)]
    pub(crate) struct KeyBindingDef {
        pub(crate) key: String,
        pub(crate) ctrl: Option<bool>,
        pub(crate) alt: Option<bool>,
        pub(crate) shift: Option<bool>,
    }

    impl KeyBindingDef {
        fn to_key_binding(&self) -> KeyBinding {
            let mut modifiers = KeyModifiers::NONE;
            if self.ctrl.unwrap_or(false) { modifiers |= KeyModifiers::CONTROL; }
            if self.alt.unwrap_or(false) { modifiers |= KeyModifiers::ALT; }
            if self.shift.unwrap_or(false) { modifiers |= KeyModifiers::SHIFT; }
            let code = parse_key_code(&self.key);
            KeyBinding::new(code, modifiers)
        }
    }

    fn parse_key_code(s: &str) -> KeyCode {
        match s.to_lowercase().as_str() {
            "enter" => KeyCode::Enter,
            "tab" => KeyCode::Tab,
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "esc" | "escape" => KeyCode::Esc,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "page_up" => KeyCode::PageUp,
            "pagedown" | "page_down" => KeyCode::PageDown,
            "f1" => KeyCode::F(1),
            "f2" => KeyCode::F(2),
            "f3" => KeyCode::F(3),
            "f4" => KeyCode::F(4),
            "f5" => KeyCode::F(5),
            "f6" => KeyCode::F(6),
            "f7" => KeyCode::F(7),
            "f8" => KeyCode::F(8),
            "f9" => KeyCode::F(9),
            "f10" => KeyCode::F(10),
            "f11" => KeyCode::F(11),
            "f12" => KeyCode::F(12),
            "space" => KeyCode::Char(' '),
            c if c.len() == 1 => KeyCode::Char(c.chars().next().unwrap()),
            _ => KeyCode::Char('?'),
        }
    }

    fn parse_modifiers(s: &str) -> KeyModifiers {
        let mut mods = KeyModifiers::NONE;
        for part in s.split('+') {
            match part.trim().to_lowercase().as_str() {
                "ctrl" | "control" => mods |= KeyModifiers::CONTROL,
                "alt" | "option" => mods |= KeyModifiers::ALT,
                "shift" => mods |= KeyModifiers::SHIFT,
                "super" | "meta" | "cmd" | "command" => mods |= KeyModifiers::SUPER,
                _ => {}
            }
        }
        mods
    }

    impl MuldexConfig {
        pub(crate) fn load() -> Option<Self> {
            let path = config_path()?;
            let content = std::fs::read_to_string(&path).ok()?;
            let toml_config: MuldexConfigToml = toml::from_str(&content).ok()?;
            Some(toml_config.into())
        }

        pub(crate) fn apply_keymap(&self, keymap: &mut RuntimeKeymap) {
            if let Some(kc) = &self.keymap {
                if let Some(app) = &kc.app {
                    if let Some(v) = &app.open_transcript { keymap.app.open_transcript = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &app.open_external_editor { keymap.app.open_external_editor = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &app.copy { keymap.app.copy = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &app.clear_terminal { keymap.app.clear_terminal = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &app.toggle_vim_mode { keymap.app.toggle_vim_mode = v.iter().map(|b| b.to_key_binding()).collect(); }
                }
                if let Some(chat) = &kc.chat {
                    if let Some(v) = &chat.interrupt_turn { keymap.chat.interrupt_turn = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &chat.decrease_reasoning_effort { keymap.chat.decrease_reasoning_effort = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &chat.increase_reasoning_effort { keymap.chat.increase_reasoning_effort = v.iter().map(|b| b.to_key_binding()).collect(); }
                }
                if let Some(composer) = &kc.composer {
                    if let Some(v) = &composer.submit { keymap.composer.submit = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &composer.queue { keymap.composer.queue = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &composer.toggle_shortcuts { keymap.composer.toggle_shortcuts = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &composer.history_search_previous { keymap.composer.history_search_previous = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &composer.history_search_next { keymap.composer.history_search_next = v.iter().map(|b| b.to_key_binding()).collect(); }
                }
                if let Some(editor) = &kc.editor {
                    if let Some(v) = &editor.insert_newline { keymap.editor.insert_newline = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_left { keymap.editor.move_left = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_right { keymap.editor.move_right = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_up { keymap.editor.move_up = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_down { keymap.editor.move_down = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_word_left { keymap.editor.move_word_left = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_word_right { keymap.editor.move_word_right = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_line_start { keymap.editor.move_line_start = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.move_line_end { keymap.editor.move_line_end = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.delete_backward { keymap.editor.delete_backward = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.delete_forward { keymap.editor.delete_forward = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.delete_backward_word { keymap.editor.delete_backward_word = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.delete_forward_word { keymap.editor.delete_forward_word = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.kill_line_start { keymap.editor.kill_line_start = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.kill_whole_line { keymap.editor.kill_whole_line = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.kill_line_end { keymap.editor.kill_line_end = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &editor.yank { keymap.editor.yank = v.iter().map(|b| b.to_key_binding()).collect(); }
                }
                if let Some(pager) = &kc.pager {
                    if let Some(v) = &pager.scroll_up { keymap.pager.scroll_up = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &pager.scroll_down { keymap.pager.scroll_down = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &pager.page_up { keymap.pager.page_up = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &pager.page_down { keymap.pager.page_down = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &pager.jump_top { keymap.pager.jump_top = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &pager.jump_bottom { keymap.pager.jump_bottom = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &pager.close { keymap.pager.close = v.iter().map(|b| b.to_key_binding()).collect(); }
                }
                if let Some(approval) = &kc.approval {
                    if let Some(v) = &approval.approve { keymap.approval.approve = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &approval.approve_session { keymap.approval.approve_session = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &approval.deny { keymap.approval.deny = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &approval.cancel { keymap.approval.cancel = v.iter().map(|b| b.to_key_binding()).collect(); }
                }
                if let Some(list) = &kc.list {
                    if let Some(v) = &list.move_up { keymap.list.move_up = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &list.move_down { keymap.list.move_down = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &list.accept { keymap.list.accept = v.iter().map(|b| b.to_key_binding()).collect(); }
                    if let Some(v) = &list.cancel { keymap.list.cancel = v.iter().map(|b| b.to_key_binding()).collect(); }
                }
            }
        }
    }

    #[derive(Debug, serde::Deserialize)]
    struct MuldexConfigToml {
        keymap: Option<KeymapConfigToml>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct KeymapConfigToml {
        app: Option<AppKeymapOverrideToml>,
        chat: Option<ChatKeymapOverrideToml>,
        composer: Option<ComposerKeymapOverrideToml>,
        editor: Option<EditorKeymapOverrideToml>,
        pager: Option<PagerKeymapOverrideToml>,
        approval: Option<ApprovalKeymapOverrideToml>,
        list: Option<ListKeymapOverrideToml>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct AppKeymapOverrideToml {
        open_transcript: Option<Vec<KeyBindingDef>>,
        open_external_editor: Option<Vec<KeyBindingDef>>,
        copy: Option<Vec<KeyBindingDef>>,
        clear_terminal: Option<Vec<KeyBindingDef>>,
        toggle_vim_mode: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct ChatKeymapOverrideToml {
        interrupt_turn: Option<Vec<KeyBindingDef>>,
        decrease_reasoning_effort: Option<Vec<KeyBindingDef>>,
        increase_reasoning_effort: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct ComposerKeymapOverrideToml {
        submit: Option<Vec<KeyBindingDef>>,
        queue: Option<Vec<KeyBindingDef>>,
        toggle_shortcuts: Option<Vec<KeyBindingDef>>,
        history_search_previous: Option<Vec<KeyBindingDef>>,
        history_search_next: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct EditorKeymapOverrideToml {
        insert_newline: Option<Vec<KeyBindingDef>>,
        move_left: Option<Vec<KeyBindingDef>>,
        move_right: Option<Vec<KeyBindingDef>>,
        move_up: Option<Vec<KeyBindingDef>>,
        move_down: Option<Vec<KeyBindingDef>>,
        move_word_left: Option<Vec<KeyBindingDef>>,
        move_word_right: Option<Vec<KeyBindingDef>>,
        move_line_start: Option<Vec<KeyBindingDef>>,
        move_line_end: Option<Vec<KeyBindingDef>>,
        delete_backward: Option<Vec<KeyBindingDef>>,
        delete_forward: Option<Vec<KeyBindingDef>>,
        delete_backward_word: Option<Vec<KeyBindingDef>>,
        delete_forward_word: Option<Vec<KeyBindingDef>>,
        kill_line_start: Option<Vec<KeyBindingDef>>,
        kill_whole_line: Option<Vec<KeyBindingDef>>,
        kill_line_end: Option<Vec<KeyBindingDef>>,
        yank: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct PagerKeymapOverrideToml {
        scroll_up: Option<Vec<KeyBindingDef>>,
        scroll_down: Option<Vec<KeyBindingDef>>,
        page_up: Option<Vec<KeyBindingDef>>,
        page_down: Option<Vec<KeyBindingDef>>,
        jump_top: Option<Vec<KeyBindingDef>>,
        jump_bottom: Option<Vec<KeyBindingDef>>,
        close: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct ApprovalKeymapOverrideToml {
        approve: Option<Vec<KeyBindingDef>>,
        approve_session: Option<Vec<KeyBindingDef>>,
        deny: Option<Vec<KeyBindingDef>>,
        cancel: Option<Vec<KeyBindingDef>>,
    }

    #[derive(Debug, serde::Deserialize)]
    struct ListKeymapOverrideToml {
        move_up: Option<Vec<KeyBindingDef>>,
        move_down: Option<Vec<KeyBindingDef>>,
        accept: Option<Vec<KeyBindingDef>>,
        cancel: Option<Vec<KeyBindingDef>>,
    }

    impl From<MuldexConfigToml> for MuldexConfig {
        fn from(t: MuldexConfigToml) -> Self {
            Self {
                keymap: t.keymap.map(|k| KeymapConfig {
                    app: k.app.map(|a| AppKeymapOverride {
                        open_transcript: a.open_transcript,
                        open_external_editor: a.open_external_editor,
                        copy: a.copy,
                        clear_terminal: a.clear_terminal,
                        toggle_vim_mode: a.toggle_vim_mode,
                    }),
                    chat: k.chat.map(|c| ChatKeymapOverride {
                        interrupt_turn: c.interrupt_turn,
                        decrease_reasoning_effort: c.decrease_reasoning_effort,
                        increase_reasoning_effort: c.increase_reasoning_effort,
                    }),
                    composer: k.composer.map(|c| ComposerKeymapOverride {
                        submit: c.submit,
                        queue: c.queue,
                        toggle_shortcuts: c.toggle_shortcuts,
                        history_search_previous: c.history_search_previous,
                        history_search_next: c.history_search_next,
                    }),
                    editor: k.editor.map(|e| EditorKeymapOverride {
                        insert_newline: e.insert_newline,
                        move_left: e.move_left,
                        move_right: e.move_right,
                        move_up: e.move_up,
                        move_down: e.move_down,
                        move_word_left: e.move_word_left,
                        move_word_right: e.move_word_right,
                        move_line_start: e.move_line_start,
                        move_line_end: e.move_line_end,
                        delete_backward: e.delete_backward,
                        delete_forward: e.delete_forward,
                        delete_backward_word: e.delete_backward_word,
                        delete_forward_word: e.delete_forward_word,
                        kill_line_start: e.kill_line_start,
                        kill_whole_line: e.kill_whole_line,
                        kill_line_end: e.kill_line_end,
                        yank: e.yank,
                    }),
                    pager: k.pager.map(|p| PagerKeymapOverride {
                        scroll_up: p.scroll_up,
                        scroll_down: p.scroll_down,
                        page_up: p.page_up,
                        page_down: p.page_down,
                        jump_top: p.jump_top,
                        jump_bottom: p.jump_bottom,
                        close: p.close,
                    }),
                    approval: k.approval.map(|a| ApprovalKeymapOverride {
                        approve: a.approve,
                        approve_session: a.approve_session,
                        deny: a.deny,
                        cancel: a.cancel,
                    }),
                    list: k.list.map(|l| ListKeymapOverride {
                        move_up: l.move_up,
                        move_down: l.move_down,
                        accept: l.accept,
                        cancel: l.cancel,
                    }),
                }),
            }
        }
    }

    pub(crate) fn config_path() -> Option<PathBuf> {
dirs::config_dir().map(|mut p| {
            p.push("muldex");
            p.push("config.toml");
            p
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use muldex_core::protocol::StateChangeKind;
    use muldex_runtime::continuity::ExportedReportView;
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::sync::OnceLock;

    fn config_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    fn cleanup(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
    }

    fn sample_state() -> RuntimeState {
        RuntimeState {
            request: ContinueRequest {
                thread_id: "thread-sample".to_string(),
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

    #[test]
    fn llm_router_config_commands_persist_user_config() {
        let _guard = config_env_lock().lock().expect("config env lock");
        let config_path = temp_root("muldex-cli-config.json");
        let _ = std::fs::remove_file(&config_path);
        unsafe {
            std::env::set_var("MULDEX_CONFIG_PATH", &config_path);
        }

        let message = handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetHost(
            "127.0.0.1".to_string(),
        ))
        .expect("set host");
        assert!(message.contains("llm-router host set to 127.0.0.1"));

        handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetPort(3000))
            .expect("set port");
        handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetApiKey(
            "secret-key".to_string(),
        ))
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
        let provider = loaded
            .providers
            .get("openai-prod")
            .expect("provider present");
        assert_eq!(
            provider.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
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

        let message = handle_interactive_provider_command(InteractiveProviderCommand::Use(
            "openai-prod".to_string(),
        ))
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

        handle_interactive_llm_config_command(InteractiveLlmConfigCommand::SetHost(
            "127.0.0.1".to_string(),
        ))
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
    fn interactive_key_handler_submits_multiline_slash_buffer_as_prompt() {
        let mut buffer = InteractivePromptBuffer {
            text: "/model\nsecond line".to_string(),
            cursor: "/model\nsecond line".len(),
        };
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
    fn interactive_key_handler_ignores_key_release_events() {
        let mut buffer = InteractivePromptBuffer::default();
        let mut completion = InteractiveSlashCompletionState::default();
        let mut history = InteractiveHistoryState::default();
        let mut search = InteractiveHistorySearchState::default();
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent {
                code: KeyCode::Char('e'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Release,
                state: crossterm::event::KeyEventState::NONE,
            },
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
        );

        assert_eq!(action, InteractiveKeyAction::Noop);
        assert!(buffer.text.is_empty());
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
        );
        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(completion.current_command(), Some("/status"));

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let _ = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
        );
        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let _ = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
        );
        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
        );

        assert_eq!(action, InteractiveKeyAction::RedrawFrame);
        assert_eq!(buffer.first_line(), "/status");
    }

    #[test]
    fn interactive_history_state_replays_and_restores_draft() {
        let mut history =
            InteractiveHistoryState::from_entries(vec!["first".to_string(), "second".to_string()]);
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

        assert_eq!(
            history.entries,
            vec!["same".to_string(), "next".to_string()]
        );
    }

    #[test]
    fn interactive_history_search_finds_and_cycles_matches() {
        let history = InteractiveHistoryState::from_entries(vec![
            "alpha task".to_string(),
            "beta fix".to_string(),
            "alpha review".to_string(),
        ]);
        let mut search = InteractiveHistorySearchState::default();
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let _ = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
        );
        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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
        let mut overlay = interactive_tui::overlay::OverlayState::default();
        let session_id = "test-session";

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Left, KeyModifiers::ALT),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
        );
        assert_eq!(action, InteractiveKeyAction::RedrawPrompt);
        assert_eq!(buffer.cursor, "alpha ".len());

        let action = handle_interactive_key_event(
            KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut buffer,
            &mut completion,
            &mut history,
            &mut search,
            &mut overlay,
            session_id,
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

        assert!(!apply_interactive_slash_completion(
            &mut buffer,
            &mut completion
        ));
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

        assert!(apply_interactive_slash_completion(
            &mut buffer,
            &mut completion
        ));
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

        assert!(apply_interactive_slash_completion(
            &mut buffer,
            &mut completion
        ));
        assert_eq!(buffer.text, "/help");

        assert!(apply_interactive_slash_completion(
            &mut buffer,
            &mut completion
        ));
        assert_eq!(buffer.text, "/status");

        assert!(apply_interactive_slash_completion(
            &mut buffer,
            &mut completion
        ));
        assert_eq!(buffer.text, "/model");
    }

    #[test]
    fn interactive_slash_completion_only_replaces_first_line() {
        let mut buffer = InteractivePromptBuffer {
            text: "/mo\nsecond line".to_string(),
            cursor: "/mo\nsecond line".len(),
        };
        let mut completion = InteractiveSlashCompletionState::default();

        assert!(apply_interactive_slash_completion(
            &mut buffer,
            &mut completion
        ));
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

        assert!(!apply_interactive_slash_completion(
            &mut buffer,
            &mut completion
        ));
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
                muldex_runtime::runtime::RuntimeCommand::Decision(
                    muldex_core::protocol::ContinueDecision {
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
                    },
                ),
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
                muldex_runtime::runtime::RuntimeCommand::Decision(
                    muldex_core::protocol::ContinueDecision {
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
                    },
                ),
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

    #[test]
    fn active_provider_is_configured_only_when_default_provider_entry_exists() {
        let mut config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("router-a".to_string()),
            providers: BTreeMap::new(),
            llm_router: None,
        };

        assert!(!active_provider_is_configured(&config));

        config.providers.insert(
            "router-a".to_string(),
            ProviderConfig {
                kind: "openai-compatible".to_string(),
                host: Some("127.0.0.1".to_string()),
                port: Some(3000),
                base_url: None,
                api_key: None,
                api_key_env: None,
                default_model: Some("gpt-5.4".to_string()),
            },
        );

        assert!(active_provider_is_configured(&config));
    }

    #[test]
    fn active_provider_summary_returns_empty_when_default_provider_entry_missing() {
        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: Some("router-a".to_string()),
            providers: BTreeMap::new(),
            llm_router: None,
        };

        assert!(
            active_provider_summary(&config, &sample_state(), &interactive_shell_state())
                .is_empty()
        );
    }

    #[test]
    fn active_provider_name_falls_back_to_llm_router_when_default_provider_missing() {
        let config = MuldexConfig {
            schema_version: "muldex-config-v1".to_string(),
            default_provider: None,
            providers: BTreeMap::from([(
                "llm-router".to_string(),
                ProviderConfig {
                    kind: "openai-compatible".to_string(),
                    host: Some("127.0.0.1".to_string()),
                    port: Some(3000),
                    base_url: None,
                    api_key: None,
                    api_key_env: None,
                    default_model: Some("gpt-5.4".to_string()),
                },
            )]),
            llm_router: None,
        };

        assert_eq!(active_provider_name(&config), Some("llm-router".to_string()));
        assert!(active_provider_is_configured(&config));
    }

    #[test]
    fn interactive_shell_exit_notice_lines_include_resume_hint() {
        let lines = interactive_shell_exit_notice_lines("interactive-session-123");
        assert_eq!(lines[0], "leaving muldex interactive shell");
        assert!(lines[1].contains("muldex resume interactive-session-123"));
    }

    fn vim_key(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty())
    }

    fn vim_key_code(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn vim_buffer(text: &str) -> InteractivePromptBuffer {
        let mut b = InteractivePromptBuffer::default();
        b.text = text.to_string();
        b.cursor = text.len();
        b
    }

    #[test]
    fn vim_normal_mode_moves_and_toggles_insert() {
        let mut st = VimState::new(true);
        let mut b = vim_buffer("hello world");
        vim_normal_key(&mut st, vim_key('h'), &mut b);
        vim_normal_key(&mut st, vim_key('h'), &mut b);
        assert_eq!(b.cursor, "hello wor".len());
        vim_normal_key(&mut st, vim_key('i'), &mut b);
        assert!(!st.normal);
        vim_normal_key(&mut st, vim_key_code(KeyCode::Esc), &mut b);
        assert!(st.normal);
    }

    #[test]
    fn vim_word_back_motion_and_delete_word() {
        let mut st = VimState::new(true);
        let mut b = vim_buffer("the quick brown fox");
        vim_normal_key(&mut st, vim_key('b'), &mut b);
        assert_eq!(&b.text[..b.cursor], "the quick brown ");
        vim_normal_key(&mut st, vim_key('d'), &mut b);
        vim_normal_key(&mut st, vim_key('w'), &mut b);
        assert_eq!(b.text, "the quick brown ");
        assert_eq!(st.ring.last().map(|s| s.as_str()), Some("fox"));
    }

    #[test]
    fn vim_yank_word_and_put_uses_kill_ring() {
        let mut st = VimState::new(true);
        let mut b = vim_buffer("abc def");
        vim_normal_key(&mut st, vim_key('b'), &mut b);
        vim_normal_key(&mut st, vim_key('y'), &mut b);
        vim_normal_key(&mut st, vim_key('w'), &mut b);
        assert_eq!(st.ring.last().map(|s| s.as_str()), Some("def"));
        vim_normal_key(&mut st, vim_key('p'), &mut b);
        assert_eq!(b.text, "abc defdef");
    }

    #[test]
    fn vim_delete_line_removes_current_line() {
        let mut st = VimState::new(true);
        let mut b = InteractivePromptBuffer::default();
        b.text = "line one\nline two\nline three".to_string();
        b.cursor = b.text.find("line two").unwrap() + 3;
        vim_normal_key(&mut st, vim_key('d'), &mut b);
        vim_normal_key(&mut st, vim_key('d'), &mut b);
        assert_eq!(b.text, "line one\nline three");
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
    println!(
        "snapshot.show_raw_agent_reasoning: {}",
        snapshot.show_raw_agent_reasoning
    );
    println!(
        "snapshot.reference_context: {}",
        snapshot.reference_context_present
    );
    println!("snapshot.input_modalities: {:?}", snapshot.input_modalities);
    println!("snapshot.tools_visible: {}", snapshot.tools_visible_count);
}

fn print_live_snapshot_summary(snapshot: &CodexLiveContinuationSnapshot) {
    println!("snapshot.kind: codex-live");
    println!("snapshot.thread_id: {}", snapshot.thread_id);
    println!(
        "snapshot.active_turn_present: {}",
        snapshot.active_turn_present
    );
    println!(
        "snapshot.pending_input_present: {}",
        snapshot.pending_input_present
    );
    println!(
        "snapshot.trigger_turn_mailbox_present: {}",
        snapshot.trigger_turn_mailbox_present
    );
    println!(
        "snapshot.auto_compact_window_number: {}",
        snapshot.auto_compact_window_number
    );
    println!(
        "snapshot.total_input_tokens: {:?}",
        snapshot.total_input_tokens
    );
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
        return Err(format!(
            "workspace does not exist or is not a directory: {}",
            workspace.display()
        )
        .into());
    }

    let objective = match (objective, objective_file) {
        (Some(text), None) => text,
        (None, Some(path)) => fs::read_to_string(path)?,
        (Some(_), Some(_)) => {
            return Err("provide either --objective or --objective-file, not both".into());
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
        request.self_correction.last_correction_target =
            Some("retry failed step in real workspace".to_string());
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
                },
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
