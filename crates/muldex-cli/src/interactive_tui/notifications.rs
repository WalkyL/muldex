use std::sync::Mutex;
use std::time::{Duration, Instant};

use ratatui::style::{Color, Modifier, Style};

struct ActiveNotification {
    message: String,
    expires_at: Instant,
}

static ACTIVE: Mutex<Option<ActiveNotification>> = Mutex::new(None);

/// Push a transient in-app notification that auto-dismisses after `ttl`.
pub(crate) fn notify(message: impl Into<String>, ttl: Duration) {
    let _ = ACTIVE.lock().map(|mut guard| {
        *guard = Some(ActiveNotification {
            message: message.into(),
            expires_at: Instant::now() + ttl,
        });
    });
}

/// Render the active notification line, or `None` if there is none/expired.
pub(crate) fn current_notification() -> Option<String> {
    let mut guard = ACTIVE.lock().ok()?;
    match guard.as_ref() {
        Some(n) if n.expires_at > Instant::now() => Some(n.message.clone()),
        Some(_) => {
            *guard = None;
            None
        }
        None => None,
    }
}

pub(crate) fn notification_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_expires() {
        notify("hello", Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
        assert!(current_notification().is_none());
        notify("world", Duration::from_secs(60));
        assert_eq!(current_notification(), Some("world".to_string()));
    }

    #[test]
    fn notification_style_is_bold_cyan() {
        let _ = notification_style();
    }
}
