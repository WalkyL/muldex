use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use portable_pty::{CommandBuilder, PtySize, native_pty_system};


fn binary_path() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_BIN_EXE_muldex").expect("muldex binary path"))
}

fn temp_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis();
    std::env::temp_dir().join(format!("muldex-e2e-{name}-{unique}.json"))
}

/// Real llm-router endpoint used for end-to-end validation.
const ROUTER_BASE_URL: &str = "http://192.168.1.44:8787/v1";

fn write_config(path: &PathBuf) {
    let cfg = serde_json::json!({
        "schema_version": "muldex-config-v1",
        "default_provider": "llm-router",
        "providers": { "llm-router": { "kind": "openai-compatible", "base_url": ROUTER_BASE_URL, "api_key": null, "default_model": "gpt-5.4" } }
    });
    std::fs::write(path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            for c in chars.by_ref() {
                if c == 'm' || c == 'R' {
                    break;
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Launch the real binary against the router, type "hi", and verify the
/// assistant reply is produced and persisted (session store is the source of
/// truth — PTY render capture is non-deterministic under headless PTY).
#[test]
#[ignore = "requires reachable llm-router at 192.168.1.44:8787"]
fn e2e_typing_hi_streams_reply() {
    let shell = temp_path("shell");
    let cfg = temp_path("cfg");
    write_config(&cfg);
    let _ = std::fs::remove_file(&shell);

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
        .expect("openpty");
    let portable_pty::PtyPair { master, slave } = pair;

    let mut cmd = CommandBuilder::new(binary_path());
    cmd.env("MULDEX_INTERACTIVE_SHELL_PATH", shell.to_string_lossy().into_owned());
    cmd.env("MULDEX_CONFIG_PATH", cfg.to_string_lossy().into_owned());
    let mut child = slave.spawn_command(cmd).expect("spawn");
    drop(slave);

    let mut reader = master.try_clone_reader().expect("reader");
    let mut writer = master.take_writer().expect("writer");
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => { let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string()); }
                Err(_) => break,
            }
        }
    });

    // wait for the TUI to render the transcript
    let mut out = String::new();
    let mut cpr = false;
    for _ in 0..80 {
        if let Ok(c) = rx.recv_timeout(Duration::from_millis(300)) {
            out.push_str(&c);
            if !cpr && c.contains("\u{1b}[6n") {
                writer.write_all(b"\x1b[1;1R").unwrap();
                writer.flush().unwrap();
                cpr = true;
            }
            if out.contains("Transcript") {
                break;
            }
        }
    }
    assert!(out.contains("Transcript"), "no transcript rendered: {out}");

    // type "hi" then Enter
    writer.write_all(b"hi").unwrap();
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(300));
    writer.write_all(b"\r").unwrap();
    writer.flush().unwrap();

    // poll the session store until an Assistant reply is persisted
    let mut replied = false;
    let mut snapshot = String::new();
    for _ in 0..240 {
        if let Ok(text) = std::fs::read_to_string(&shell) {
            snapshot = text.clone();
            // final assistant message is stored as {"role":"Assistant","content": ...}
            if let Some(idx) = text.find("\"role\": \"Assistant\"") {
                let tail = &text[idx..];
                if let Some(content_start) = tail.find("\"content\": \"") {
                    let start = content_start + "\"content\": \"".len();
                    let content = &tail[start..];
                    let end = content.find("\"").unwrap_or(content.len());
                    if !content[..end].is_empty() {
                        replied = true;
                        break;
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(250));
    }

    let exit_status = child.try_wait().expect("try_wait");
    let really_exited = exit_status.is_some();
    eprintln!("=== REPLIED? {} | PROCESS_EXITED? {} ===", replied, really_exited);
    eprintln!("=== STORE TAIL ===\n{}", &strip_ansi(&snapshot)[snapshot.len().saturating_sub(800)..]);

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&shell);
    let _ = std::fs::remove_file(&cfg);

    assert!(!really_exited, "binary process exited during turn");
    assert!(replied, "no assistant reply persisted to session store");
}

/// Verify the transcript pager overlay opens (Ctrl+T) and closes (Esc) without
/// breaking the interactive loop (a subsequent prompt still gets a reply).
#[test]
#[ignore = "requires reachable llm-router at 192.168.1.44:8787"]
fn e2e_transcript_pager_opens_and_closes() {
    let shell = temp_path("shell");
    let cfg = temp_path("cfg");
    write_config(&cfg);
    let _ = std::fs::remove_file(&shell);

    let pty = native_pty_system();
    let pair = pty
        .openpty(PtySize { rows: 30, cols: 100, pixel_width: 0, pixel_height: 0 })
        .expect("openpty");
    let portable_pty::PtyPair { master, slave } = pair;

    let mut cmd = CommandBuilder::new(binary_path());
    cmd.env("MULDEX_INTERACTIVE_SHELL_PATH", shell.to_string_lossy().into_owned());
    cmd.env("MULDEX_CONFIG_PATH", cfg.to_string_lossy().into_owned());
    let mut child = slave.spawn_command(cmd).expect("spawn");
    drop(slave);

    let mut reader = master.try_clone_reader().expect("reader");
    let mut writer = master.take_writer().expect("writer");
    let (tx, rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => { let _ = tx.send(String::from_utf8_lossy(&buf[..n]).to_string()); }
                Err(_) => break,
            }
        }
    });

    let mut out = String::new();
    let mut cpr = false;
    for _ in 0..80 {
        if let Ok(c) = rx.recv_timeout(Duration::from_millis(300)) {
            out.push_str(&c);
            if !cpr && c.contains("\u{1b}[6n") {
                writer.write_all(b"\x1b[1;1R").unwrap();
                writer.flush().unwrap();
                cpr = true;
            }
            if out.contains("Transcript") {
                break;
            }
        }
    }
    assert!(out.contains("Transcript"), "no transcript rendered: {out}");

    // open transcript pager, then close it
    writer.write_all(b"\x14").unwrap(); // Ctrl+T
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(400));
    writer.write_all(b"\x1b").unwrap(); // Esc
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(400));
    assert!(
        child.try_wait().expect("try_wait").is_none(),
        "binary process exited while opening/closing pager"
    );

    // a follow-up prompt must still produce a reply (loop intact)
    writer.write_all(b"hello").unwrap();
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(300));
    writer.write_all(b"\r").unwrap();
    writer.flush().unwrap();

    let mut replied = false;
    for _ in 0..120 {
        if let Ok(text) = std::fs::read_to_string(&shell) {
            if let Some(idx) = text.find("\"role\": \"Assistant\"") {
                let tail = &text[idx..];
                if let Some(content_start) = tail.find("\"content\": \"") {
                    let start = content_start + "\"content\": \"".len();
                    let content = &tail[start..];
                    let end = content.find("\"").unwrap_or(content.len());
                    if !content[..end].is_empty() {
                        replied = true;
                        break;
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(250));
    }

    eprintln!("=== PAGER ROUND-TRIP REPLIED? {} ===", replied);

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&shell);
    let _ = std::fs::remove_file(&cfg);

    assert!(replied, "follow-up prompt after pager close produced no reply");
}
