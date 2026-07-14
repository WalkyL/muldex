use ratatui::style::{Color, Modifier, Style};

use super::terminal_palette;

pub(crate) const TRANSCRIPT_TITLE: &str = "Transcript";
pub(crate) const STATUS_TITLE: &str = "Status";
pub(crate) const COMPOSER_TITLE: &str = "Composer";
pub(crate) const SLASH_TITLE: &str = "Slash";
pub(crate) const SEARCH_TITLE: &str = "Search";
pub(crate) const EMPTY_TRANSCRIPT: &str = "No messages yet. Start with a prompt or slash command.";
pub(crate) const PROVIDER_NOT_CONFIGURED: &str = "provider not configured";

fn terminal_bg() -> Option<(u8, u8, u8)> {
    terminal_palette::default_bg()
}

pub(crate) fn top_bar() -> Style {
    let bg = terminal_bg();
    let accent = terminal_palette::accent_style_for(bg);
    Style::default()
        .fg(Color::Black)
        .bg(accent)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn block_border() -> Style {
    let bg = terminal_bg();
    match bg {
        Some(_) => Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM),
        None => Style::default().fg(Color::DarkGray),
    }
}

pub(crate) fn section_title() -> Style {
    let bg = terminal_bg();
    let accent = terminal_palette::accent_style_for(bg);
    Style::default()
        .fg(accent)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn transcript_role(role: &str) -> Style {
    match role {
        "SYSTEM" => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        "USER" => Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
        "ASSISTANT" => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        "TOOL" => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::DIM),
        "APPROVAL" => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    }
}

pub(crate) fn hint_selected() -> Style {
    let bg = terminal_bg();
    match bg {
        Some(rgb) if terminal_palette::is_dark_bg() => Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
        _ => Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    }
}

pub(crate) fn hint_normal() -> Style {
    Style::default().fg(Color::Gray)
}

pub(crate) fn placeholder() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC)
}

pub(crate) fn status_label() -> Style {
    let bg = terminal_bg();
    let accent = terminal_palette::accent_style_for(bg);
    Style::default()
        .fg(accent)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn composer_prefix() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn user_message_style() -> Style {
    match terminal_bg() {
        Some(bg) => Style::default().bg(terminal_palette::user_message_bg(bg)),
        None => Style::default(),
    }
}

pub(crate) fn error_style() -> Style {
    Style::default().fg(Color::Red)
}

pub(crate) fn success_style() -> Style {
    Style::default().fg(Color::Green)
}

pub(crate) fn dim_style() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

pub(crate) fn footer_style() -> Style {
    let bg = terminal_bg();
    let accent = terminal_palette::accent_style_for(bg);
    Style::default().fg(accent).bg(Color::Reset)
}

pub(crate) fn footer_badge_idle() -> Style {
    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
}

pub(crate) fn footer_badge_thinking() -> Style {
    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
}

pub(crate) fn footer_badge_busy() -> Style {
    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
}

pub(crate) fn footer_hint() -> Style {
    Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM)
}

pub(crate) fn footer_token_summary() -> Style {
    Style::default().fg(Color::Magenta).add_modifier(Modifier::DIM)
}

pub(crate) fn markdown_heading() -> Color {
    Color::Cyan
}

pub(crate) fn markdown_bullet() -> Color {
    Color::Cyan
}

pub(crate) fn markdown_quote() -> Color {
    Color::Magenta
}

pub(crate) fn markdown_link() -> Color {
    Color::Blue
}

pub(crate) fn markdown_code() -> Color {
    Color::Gray
}

pub(crate) fn markdown_inline_code() -> Color {
    Color::Yellow
}

pub(crate) fn markdown_code_border() -> Color {
    Color::DarkGray
}

pub(crate) fn overlay_block() -> Style {
    Style::default().fg(Color::Cyan)
}

pub(crate) fn overlay_hint() -> Style {
    Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM)
}

pub(crate) fn approval_block() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn approval_hint() -> Style {
    Style::default().fg(Color::Magenta).add_modifier(Modifier::DIM)
}

pub(crate) fn vim_normal_mode() -> Style {
    Style::default()
        .fg(Color::Magenta)
        .add_modifier(Modifier::BOLD)
}

pub(crate) fn vim_insert_mode() -> Style {
    Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD)
}