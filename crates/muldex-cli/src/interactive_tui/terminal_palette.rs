use std::io::{Read, Write};
use std::sync::OnceLock;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use ratatui::style::Color;

static DETECTED_BG: OnceLock<Option<(u8, u8, u8)>> = OnceLock::new();

fn is_light(rgb: (u8, u8, u8)) -> bool {
    let (r, g, b) = rgb;
    let luminance = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    luminance > 128.0
}

pub(crate) fn set_terminal_bg(rgb: (u8, u8, u8)) {
    let _ = DETECTED_BG.set(Some(rgb));
}

pub(crate) fn detect_terminal_bg() -> Option<(u8, u8, u8)> {
    *DETECTED_BG.get_or_init(|| None)
}

pub(crate) fn default_bg() -> Option<(u8, u8, u8)> {
    detect_terminal_bg()
}

pub(crate) fn is_dark_bg() -> bool {
    match default_bg() {
        Some(bg) => !is_light(bg),
        None => true,
    }
}

/// Best-effort terminal background detection via the OSC 11 query
/// (`\x1b]11;?\x1b\\`). Terminals that support it reply with
/// `\x1b]11;rgb:r/g/b\x1b\\`. We read raw stdin with a short timeout and
/// never block the UI if the terminal stays silent.
pub(crate) fn probe_background_color() -> Option<(u8, u8, u8)> {
    let _ = write!(std::io::stdout(), "\x1b]11;?\x1b\\");
    let _ = std::io::stdout().flush();

    let (tx, rx) = mpsc::channel::<Option<(u8, u8, u8)>>();
    let handle = thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut collected: Vec<u8> = Vec::new();
        let mut tmp = [0u8; 64];
        loop {
            match stdin.read(&mut tmp) {
                Ok(0) => break,
                Ok(n) => {
                    collected.extend_from_slice(&tmp[..n]);
                    if let Some(color) = parse_osc11(&collected) {
                        let _ = tx.send(Some(color));
                        return;
                    }
                    if collected.len() > 512 {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = tx.send(None);
    });

    match rx.recv_timeout(Duration::from_millis(200)) {
        Ok(color) => {
            let _ = handle.join();
            color
        }
        Err(_) => {
            // abandon the probe thread; it will be reaped on process exit
            None
        }
    }
}

fn parse_osc11(bytes: &[u8]) -> Option<(u8, u8, u8)> {
    let text = String::from_utf8_lossy(bytes);
    let start = text.find("\x1b]11;")?;
    let rest = &text[start + 5..];
    let end = rest.find("\x1b\\").or_else(|| rest.find('\u{1b}'))?;
    let body = &rest[..end];
    let rgb = body.strip_prefix("rgb:")?;
    let mut parts = rgb.split('/');
    let r = parse_color_channel(parts.next()?)?;
    let g = parse_color_channel(parts.next()?)?;
    let b = parse_color_channel(parts.next()?)?;
    Some((r, g, b))
}

fn parse_color_channel(value: &str) -> Option<u8> {
    let value = value.trim().trim_end_matches('m');
    let hex = value.strip_prefix('#').unwrap_or(value);
    let digits = hex.chars().count();
    let raw = u32::from_str_radix(hex, 16).ok()?;
    // OSC 11 / XParseColor channels vary in width: 2-digit is 0-255,
    // 4-digit is 16-bit (0-65535), 6-digit is 24-bit (0-16777215).
    let byte: u32 = match digits {
        1 => raw * 0x11,
        2 => raw,
        4 => raw / 257,
        6 => raw * 255 / 16_777_215,
        _ => {
            let rep: String = if digits < 4 {
                let mut s = hex.to_string();
                while s.len() < 4 {
                    s.push_str(hex);
                }
                s[..4].to_string()
            } else {
                hex.to_string()
            };
            let v = u32::from_str_radix(&rep, 16).ok()?;
            v / 257
        }
    };
    Some(byte.clamp(0, 255) as u8)
}

pub(crate) fn best_color(rgb: (u8, u8, u8)) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

fn blend(fg: (u8, u8, u8), bg: (u8, u8, u8), alpha: f32) -> (u8, u8, u8) {
    let r = (fg.0 as f32 * alpha + bg.0 as f32 * (1.0 - alpha)) as u8;
    let g = (fg.1 as f32 * alpha + bg.1 as f32 * (1.0 - alpha)) as u8;
    let b = (fg.2 as f32 * alpha + bg.2 as f32 * (1.0 - alpha)) as u8;
    (r, g, b)
}

pub(crate) fn user_message_bg(terminal_bg: (u8, u8, u8)) -> Color {
    let (top, alpha) = if is_light(terminal_bg) {
        ((0, 0, 0), 0.04)
    } else {
        ((255, 255, 255), 0.12)
    };
    best_color(blend(top, terminal_bg, alpha))
}

pub(crate) fn accent_style_for(terminal_bg: Option<(u8, u8, u8)>) -> Color {
    match terminal_bg {
        Some(bg) if is_light(bg) => best_color((0, 95, 135)),
        _ => Color::Cyan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_background_detection() {
        assert!(is_light((200, 200, 200)));
        assert!(!is_light((50, 50, 50)));
    }

    #[test]
    fn dark_is_default_when_no_bg_detected() {
        assert!(is_dark_bg());
    }

    #[test]
    fn accent_style_defaults_to_cyan() {
        let color = accent_style_for(None);
        assert_eq!(color, Color::Cyan);
    }

    #[test]
    fn parses_osc11_rgb_response() {
        let bytes = b"\x1b]11;rgb:1e/1e/1b\x1b\\";
        assert_eq!(parse_osc11(bytes), Some((30, 30, 27)));
        let bytes2 = b"garbage\x1b]11;rgb:2a2a/2a2a/2a2a\x1b\\more";
        assert_eq!(parse_osc11(bytes2), Some((42, 42, 42)));
    }
}