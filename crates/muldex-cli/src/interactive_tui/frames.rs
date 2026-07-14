use ratatui::style::Style;
use ratatui::symbols::border;
use ratatui::text::Line;
use ratatui::widgets::Block;

use super::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameStyle {
    Codex,
    Default,
    Minimal,
}

impl FrameStyle {
    pub(crate) fn titled_block(self, title: &str) -> Block<'static> {
        self.titled_block_line(Line::from(ratatui::text::Span::styled(
            title.to_string(),
            theme::section_title(),
        )))
    }

    /// Like `titled_block`, but the title carries its own (caller-supplied)
    /// styling — used for the composer mode indicator.
    pub(crate) fn titled_block_line(self, title: Line<'static>) -> Block<'static> {
        match self {
            FrameStyle::Codex => Block::default()
                .title(title)
                .borders(ratatui::widgets::Borders::ALL)
                .border_set(border::ROUNDED)
                .border_style(theme::block_border()),
            FrameStyle::Default => Block::default()
                .title(title)
                .borders(ratatui::widgets::Borders::ALL)
                .border_set(border::PLAIN)
                .border_style(theme::block_border()),
            FrameStyle::Minimal => Block::default()
                .title(title)
                .borders(ratatui::widgets::Borders::TOP)
                .border_set(border::PLAIN)
                .border_style(theme::block_border()),
        }
    }
}