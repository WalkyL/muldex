use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
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
    std::env::temp_dir().join(format!("muldex-repro-{name}-{unique}.json"))
}

struct MockProviderServer {
    address: String,
    stop: Arc<Mutex<bool>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MockProviderServer {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let address = listener.local_addr().expect("mock provider addr").to_string();
        let stop = Arc::new(Mutex::new(false));
        let stop_flag = stop.clone();
        let handle = thread::spawn(move || {
            while !*stop_flag.lock().expect("stop flag") {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let body = read_http_body(&mut stream);
                        let resp = mock_response(&body);
                        let headers = concat!(
                            "HTTP/1.1 200 OK\r\n",
                            "content-type: text/event-stream\r\n",
                            "transfer-encoding: chunked\r\n\r\n"
                        );
                        stream.write_all(headers.as_bytes()).expect("headers");
                        let chunk = format!("{:X}\r\n{}\r\n0\r\n\r\n", resp.len(), resp);
                        stream.write_all(chunk.as_bytes()).expect("body");
                        stream.flush().expect("flush");
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });
        Self { address, stop, handle: Some(handle) }
    }
    fn base_url(&self) -> String {
        format!("http://{}/v1", self.address)
    }
}

impl Drop for MockProviderServer {
    fn drop(&mut self) {
        *self.stop.lock().expect("stop flag") = true;
        let _ = std::net::TcpStream::connect(&self.address);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn read_http_body(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut header_end = None;
    let mut scratch = [0u8; 4096];
    while header_end.is_none() {
        let bytes = match stream.read(&mut scratch) {
            Ok(b) if b > 0 => b,
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(_) => break,
        };
        buffer.extend_from_slice(&scratch[..bytes]);
        header_end = buffer.windows(4).position(|w| w == b"\r\n\r\n");
    }
    let header_end = header_end.expect("hdr") + 4;
    let header_text = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let cl = header_text
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(0);
    while buffer.len().saturating_sub(header_end) < cl {
        let bytes = match stream.read(&mut scratch) {
            Ok(b) if b > 0 => b,
            Ok(_) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(_) => break,
        };
        buffer.extend_from_slice(&scratch[..bytes]);
    }
    String::from_utf8_lossy(&buffer[header_end..header_end + cl]).to_string()
}

fn mock_response(body: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::Value::Null);
    let last_user = v["input"]
        .as_array()
        .and_then(|a| {
            a.iter()
                .rev()
                .find(|i| i["type"] == "message" && i["role"] == "user")
                .and_then(|i| i["content"].as_array())
                .and_then(|c| c.first())
                .and_then(|p| p["text"].as_str())
        })
        .unwrap_or("");
    let reply = format!("hi back: {}", last_user.replace('\\', "\\\\").replace('"', "\\\""));
    format!(
        "event: response.output_text.delta\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":\"{}\",\"item_id\":\"i1\",\"output_index\":0,\"content_index\":0}}\n\nevent: response.output_text.done\ndata: {{\"type\":\"response.output_text.done\",\"text\":\"{}\",\"item_id\":\"i1\"}}\n\nevent: response.completed\ndata: {{\"type\":\"response.completed\"}}\n\n",
        reply, reply
    )
}

fn write_config(path: &PathBuf, base_url: &str) {
    let cfg = serde_json::json!({
        "schema_version": "muldex-config-v1",
        "default_provider": "llm-router",
        "providers": { "llm-router": { "kind": "openai-compatible", "base_url": base_url, "api_key": "k", "default_model": "gpt-5-test" } }
    });
    std::fs::write(path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
}

#[test]
fn repro_tty_typing_hi() {
    let shell = temp_path("shell");
    let cfg = temp_path("cfg");
    let server = MockProviderServer::start();
    write_config(&cfg, &server.base_url());
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
    for _ in 0..60 {
        if let Ok(c) = rx.recv_timeout(Duration::from_millis(200)) {
            out.push_str(&c);
            if !cpr && c.contains("\u{1b}[6n") {
                writer.write_all(b"\x1b[1;1R").unwrap();
                writer.flush().unwrap();
                cpr = true;
            }
            if out.contains("Transcript") { break; }
        }
    }

    // type "hi" then Enter (without ctrl-c)
    writer.write_all(b"hi").unwrap();
    writer.flush().unwrap();
    thread::sleep(Duration::from_millis(300));
    writer.write_all(b"\r").unwrap();
    writer.flush().unwrap();

    // capture for a while
    let mut after = String::new();
    let mut exited = false;
    for _ in 0..120 {
        if let Ok(c) = rx.recv_timeout(Duration::from_millis(200)) {
            after.push_str(&c);
            if c.contains("leaving muldex") || c.contains("To continue this session") {
                exited = true;
                break;
            }
        }
    }

    eprintln!("=== BEFORE TYPING (last 1500) ===\n{}", &out[out.len().saturating_sub(1500)..]);
    eprintln!("=== AFTER TYPING hi+Enter (last 3000) ===\n{}", &after[after.len().saturating_sub(3000)..]);
    eprintln!("=== EXITED? {} ===", exited);

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&shell);
    let _ = std::fs::remove_file(&cfg);

    assert!(out.contains("Transcript"), "no transcript rendered: {out}");
}
