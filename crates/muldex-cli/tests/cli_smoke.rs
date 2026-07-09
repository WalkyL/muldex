use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};
use std::io::Write;
use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use muldex_runtime::client_views::ClientResponsePayloadView;
use muldex_runtime::client_views::ClientResponseView;
use muldex_runtime::client_views::ClientDaemonStatusView;
use muldex_runtime::client_views::ClientSessionListView;
use muldex_runtime::continuity::ExportedReportView;
use muldex_runtime::continuity::ExportedSessionView;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

fn binary_path() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_muldex").expect("muldex binary path"))
}

fn temp_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis();
    std::env::temp_dir().join(format!("muldex-cli-smoke-{name}-{unique}.json"))
}

fn run_ok(args: &[&str]) -> String {
    let output = Command::new(binary_path())
        .args(args)
        .output()
        .expect("run binary");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

fn run_ok_with_stdin_and_env(args: &[&str], stdin_text: &str, envs: &[(&str, &str)]) -> String {
    let mut command = Command::new(binary_path());
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("spawn binary");
    child
        .stdin
        .as_mut()
        .expect("stdin handle")
        .write_all(stdin_text.as_bytes())
        .expect("write stdin");
    let output = child.wait_with_output().expect("wait output");
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("utf8 stdout")
}

fn cleanup_snapshot_artifacts(snapshot: &PathBuf) {
    let _ = std::fs::remove_file(snapshot);
    if let Some(parent) = snapshot.parent() {
        if let Some(stem) = snapshot.file_stem().and_then(|s| s.to_str()) {
            let _ = std::fs::remove_dir_all(parent.join(format!("{stem}.muldex-transport")));
            let _ = std::fs::remove_dir_all(parent.join(format!("{stem}.muldex-daemon")));
        }
    }
}

fn cleanup_shell_snapshot(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

fn extract_session_ids(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.strip_prefix("session.id: "))
        .map(str::trim)
        .map(str::to_string)
        .collect()
}

#[test]
fn binary_client_send_read_list_round_trip() {
    let snapshot = temp_path("snapshot");
    let command_id = "cmd-smoke-1";

    cleanup_snapshot_artifacts(&snapshot);

    run_ok(&["save-host-snapshot", "--path", snapshot.to_str().unwrap()]);

    let sessions_json = run_ok(&["client-list-sessions", "--path", snapshot.to_str().unwrap()]);
    let sessions: ClientSessionListView = serde_json::from_str(&sessions_json).expect("session list json");
    assert_eq!(sessions.session_count, 1);
    assert_eq!(sessions.sessions[0].session_id, "sample-session");

    run_ok(&[
        "client-send-command",
        "--path",
        snapshot.to_str().unwrap(),
        "--command-id",
        command_id,
        "--session-id",
        "sample-session",
        "--kind",
        "status",
        "--access-mode",
        "read-only",
    ]);

    run_ok(&[
        "server-foreground",
        "--path",
        snapshot.to_str().unwrap(),
        "--iterations",
        "1",
    ]);

    let response_json = run_ok(&[
        "client-read-response",
        "--path",
        snapshot.to_str().unwrap(),
        "--command-id",
        command_id,
    ]);
    let response: ClientResponseView = serde_json::from_str(&response_json).expect("response json");
    assert!(response.ok);
    assert_eq!(response.payload_kind, "RuntimeCommandResult");
    match response.payload.expect("payload") {
        ClientResponsePayloadView::Step { phase, cycle_index, .. } => {
            assert_eq!(phase, "Running");
            assert_eq!(cycle_index, 1);
        }
        other => panic!("unexpected payload: {:?}", other),
    }

    cleanup_snapshot_artifacts(&snapshot);
}

#[test]
fn binary_client_inspect_session_compressed_round_trip() {
    let snapshot = temp_path("inspect-compressed");
    let command_id = "cmd-smoke-2";

    cleanup_snapshot_artifacts(&snapshot);

    run_ok(&["save-host-snapshot", "--path", snapshot.to_str().unwrap()]);
    run_ok(&[
        "client-send-command",
        "--path",
        snapshot.to_str().unwrap(),
        "--command-id",
        command_id,
        "--session-id",
        "sample-session",
        "--kind",
        "status",
        "--access-mode",
        "read-only",
    ]);
    run_ok(&[
        "server-foreground",
        "--path",
        snapshot.to_str().unwrap(),
        "--iterations",
        "1",
    ]);

    let inspect_json = run_ok(&[
        "client-inspect-session",
        "--path",
        snapshot.to_str().unwrap(),
        "--session-id",
        "sample-session",
        "--mode",
        "compressed",
    ]);
    let inspect: ExportedSessionView =
        serde_json::from_str(&inspect_json).expect("inspect session json");

    assert_eq!(inspect.session_id, "sample-session");
    match inspect.report.expect("report") {
        ExportedReportView::Compressed(report) => {
            assert!(report.compressed_cycle_summary.is_some());
            assert_eq!(report.rationale, "status probe through daemon transport");
        }
        other => panic!("unexpected report view: {:?}", other),
    }

    cleanup_snapshot_artifacts(&snapshot);
}

#[test]
fn binary_client_status_returns_json_view() {
    let snapshot = temp_path("client-status");

    cleanup_snapshot_artifacts(&snapshot);
    run_ok(&["save-host-snapshot", "--path", snapshot.to_str().unwrap()]);

    let status_json = run_ok(&["client-status", "--path", snapshot.to_str().unwrap()]);
    let status: ClientDaemonStatusView =
        serde_json::from_str(&status_json).expect("client status json");

    assert_eq!(status.contract.schema_version, "client-view-v1");
    assert_eq!(status.snapshot_path, snapshot.display().to_string());
    assert_eq!(status.session_count, 0);
    assert_eq!(status.daemon_status, "Cold");
    assert_eq!(status.stale_status.as_deref(), Some("no_lock"));

    cleanup_snapshot_artifacts(&snapshot);
}

#[test]
fn binary_daemon_stale_status_reports_no_lock_before_boot() {
    let snapshot = temp_path("daemon-stale-status");

    cleanup_snapshot_artifacts(&snapshot);
    run_ok(&["save-host-snapshot", "--path", snapshot.to_str().unwrap()]);

    let stale_output = run_ok(&[
        "daemon-stale-status",
        "--path",
        snapshot.to_str().unwrap(),
        "--threshold-ms",
        "60000",
    ]);

    assert!(stale_output.contains("daemon.stale_status: no_lock"));
    assert!(stale_output.contains("daemon.threshold_ms: 60000"));

    cleanup_snapshot_artifacts(&snapshot);
}

#[test]
fn binary_default_entry_shell_accepts_exit_command() {
    let shell_snapshot = temp_path("interactive-shell-exit");
    cleanup_shell_snapshot(&shell_snapshot);
    let shell_snapshot_text = shell_snapshot.to_string_lossy().into_owned();
    let output = run_ok_with_stdin_and_env(
        &[],
        "/exit\n",
        &[("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text.as_str())],
    );
    assert!(output.contains("muldex interactive shell"));
    assert!(output.contains("== muldex session =="));
    assert!(output.contains("type /help for commands, /exit to leave"));
    assert!(output.contains("leaving muldex interactive shell"));
    cleanup_shell_snapshot(&shell_snapshot);
}

#[test]
fn binary_default_entry_shell_accepts_prompt_argument() {
    let shell_snapshot = temp_path("interactive-shell-prompt");
    cleanup_shell_snapshot(&shell_snapshot);
    let shell_snapshot_text = shell_snapshot.to_string_lossy().into_owned();
    let output = run_ok_with_stdin_and_env(
        &["hello from cli smoke"],
        "/exit\n",
        &[("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text.as_str())],
    );
    assert!(output.contains("muldex interactive shell"));
    assert!(output.contains("assistant.cycle_index: 1"));
    assert!(output.contains("assistant.summary: interactive prompt: hello from cli smoke"));
    cleanup_shell_snapshot(&shell_snapshot);
}

#[test]
fn binary_default_entry_shell_supports_codex_style_slash_commands() {
    let shell_snapshot = temp_path("interactive-shell-slash");
    cleanup_shell_snapshot(&shell_snapshot);
    let shell_snapshot_text = shell_snapshot.to_string_lossy().into_owned();
    let output = run_ok_with_stdin_and_env(
        &[],
        "/model\n/model gpt-5-mini\n/approval on-request\n/compact\n/resume\n/status\n/exit\n",
        &[("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text.as_str())],
    );
    assert!(output.contains("session.model: gpt-5.4"));
    assert!(output.contains("session.model_set: gpt-5-mini"));
    assert!(output.contains("session.approval_mode_set: on-request"));
    assert!(output.contains("session.compaction_requested: true"));
    assert!(output.contains("session.compaction_window_id: Some(\"shell-window-1\")"));
    assert!(output.contains("session.resume_requested: true"));
    assert!(output.contains("session.compact_count: 1"));
    assert!(output.contains("session.resume_count: 1"));
    assert!(output.contains("session.pending_post_compaction: true"));
    assert!(output.contains("session.first_post_compaction_turn: true"));
    assert!(output.contains("session.approval_mode: on-request"));
    cleanup_shell_snapshot(&shell_snapshot);
}

#[test]
fn binary_default_entry_shell_persists_and_resumes_session_state() {
    let shell_snapshot = temp_path("interactive-shell-resume");
    cleanup_shell_snapshot(&shell_snapshot);
    let shell_snapshot_text = shell_snapshot.to_string_lossy().into_owned();

    let first_output = run_ok_with_stdin_and_env(
        &["resume seed prompt"],
        "/model gpt-5-resume\n/approval manual\n/compact\n/new\n/model gpt-5-secondary\n/exit\n",
        &[("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text.as_str())],
    );
    assert!(first_output.contains("assistant.summary: interactive prompt: resume seed prompt"));
    assert!(first_output.contains("session.model_set: gpt-5-resume"));
    assert!(first_output.contains("session.new: true"));

    let session_ids = extract_session_ids(&first_output);
    assert!(!session_ids.is_empty());
    let resumed_session_id = session_ids.last().expect("new session id").clone();

    let second_output = run_ok_with_stdin_and_env(
        &[],
        format!("/sessions\n/resume {}\n/status\n/exit\n", resumed_session_id).as_str(),
        &[("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text.as_str())],
    );
    assert!(second_output.contains("== muldex session =="));
    assert!(second_output.contains("[system] interactive shell created"));
    assert!(second_output.contains("session.active: false"));
    assert!(second_output.contains("session.active: true"));
    assert!(second_output.contains("session.resumed: true"));
    assert!(second_output.contains(format!("session.id: {}", resumed_session_id).as_str()));
    assert!(second_output.contains("session.model: gpt-5-secondary"));
    assert!(second_output.contains("session.approval_mode: on-request"));

    cleanup_shell_snapshot(&shell_snapshot);
}

#[test]
fn binary_default_entry_shell_forced_tty_render_smoke() {
    let shell_snapshot = temp_path("interactive-shell-forced-tty");
    cleanup_shell_snapshot(&shell_snapshot);
    let shell_snapshot_text = shell_snapshot.to_string_lossy().into_owned();

    let output = run_ok_with_stdin_and_env(
        &[],
        "/model\n/exit\n",
        &[
            ("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text.as_str()),
            ("MULDEX_FORCE_TTY_RENDER", "1"),
        ],
    );

    assert!(output.contains("== muldex session =="));
    assert!(output.contains("commands: /help /status /model /approval /compact /sessions /resume /new /exit"));

    cleanup_shell_snapshot(&shell_snapshot);
}

#[test]
fn binary_default_entry_shell_scripted_reverse_search_smoke() {
    let shell_snapshot = temp_path("interactive-shell-scripted-search");
    cleanup_shell_snapshot(&shell_snapshot);
    let shell_snapshot_text = shell_snapshot.to_string_lossy().into_owned();

    let output = run_ok_with_stdin_and_env(
        &[],
        "",
        &[
            ("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text.as_str()),
            ("MULDEX_FORCE_TTY_RENDER", "1"),
            (
                "MULDEX_SCRIPTED_KEYS",
                "TEXT:search target,ENTER,CTRL_U,TEXT:se,CTRL_R,ESC,CTRL_C",
            ),
        ],
    );

    assert!(output.contains("reverse search active: se"));
    assert!(output.contains("matches: 1"));
    assert!(output.contains("match: search target"));

    cleanup_shell_snapshot(&shell_snapshot);
}

#[test]
fn binary_default_entry_shell_tty_smoke_via_pty() {
    let shell_snapshot = temp_path("interactive-shell-pty");
    cleanup_shell_snapshot(&shell_snapshot);
    let shell_snapshot_text = shell_snapshot.to_string_lossy().into_owned();

    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(PtySize {
            rows: 24,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("open pty");
    let portable_pty::PtyPair { master, slave } = pair;

    let mut cmd = CommandBuilder::new(binary_path());
    cmd.env("MULDEX_INTERACTIVE_SHELL_PATH", shell_snapshot_text);
    let mut child = slave.spawn_command(cmd).expect("spawn command");
    drop(slave);

    let mut reader = master.try_clone_reader().expect("clone reader");
    let mut writer = master.take_writer().expect("take writer");
    let (tx, rx) = mpsc::channel::<String>();

    thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(bytes) => {
                    let chunk = String::from_utf8_lossy(&buffer[..bytes]).to_string();
                    let _ = tx.send(chunk);
                }
                Err(_) => break,
            }
        }
    });

    let mut output = String::new();
    let mut responded_to_cpr = false;
    for _ in 0..40 {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            output.push_str(&chunk);
            if !responded_to_cpr && chunk.contains("\u{1b}[6n") {
                writer.write_all(b"\x1b[1;1R").expect("write cursor position response");
                writer.flush().expect("flush cursor response");
                responded_to_cpr = true;
            }
            if output.contains("== muldex session ==") {
                break;
            }
        }
    }

    writer.write_all(b"/").expect("write slash trigger");
    writer.flush().expect("flush slash trigger");
    for _ in 0..20 {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            output.push_str(&chunk);
            if output.contains("slash commands:") {
                break;
            }
        }
    }

    writer.write_all(b"\t").expect("write slash picker completion");
    writer.flush().expect("flush slash picker completion");
    for _ in 0..40 {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            output.push_str(&chunk);
            if output.contains("> /help") {
                break;
            }
        }
    }

    writer.write_all(b"\x15").expect("write ctrl-u clear line");
    writer.flush().expect("flush ctrl-u clear line");
    thread::sleep(Duration::from_millis(50));

    writer.write_all(b"/model\r").expect("write model command");
    writer.flush().expect("flush model command");
    for _ in 0..40 {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            output.push_str(&chunk);
            if output.contains("session.model: gpt-5.4") {
                break;
            }
        }
    }

    writer.write_all(b"/status\r").expect("write status command");
    writer.flush().expect("flush status command");
    for _ in 0..40 {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            output.push_str(&chunk);
            if output.contains("session.phase: Ready") {
                break;
            }
        }
    }

    writer.write_all(b"\x03").expect("write ctrl-c");
    writer.flush().expect("flush writer");
    drop(writer);

    for _ in 0..40 {
        if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
            output.push_str(&chunk);
            if output.contains("== muldex session ==") {
                break;
            }
        }
    }

    let _ = child.kill();
    let _ = child.wait();

    assert!(output.contains("== muldex session =="), "pty output: {output}");
    assert!(output.contains("session.id:"), "pty output: {output}");
    assert!(output.contains("> /help"), "pty output: {output}");
    assert!(output.contains("session.model: gpt-5.4"), "pty output: {output}");
    assert!(output.contains("session.phase: Ready"), "pty output: {output}");

    cleanup_shell_snapshot(&shell_snapshot);
}
