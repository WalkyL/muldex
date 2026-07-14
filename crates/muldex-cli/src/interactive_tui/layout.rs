use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub(crate) const TOP_BAR_HEIGHT: u16 = 1;
pub(crate) const MIN_COMPOSER_HEIGHT: u16 = 4;
pub(crate) const MAX_COMPOSER_HEIGHT: u16 = 10;
pub(crate) const STATUS_WIDTH: u16 = 38;
pub(crate) const FOOTER_HEIGHT: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShellLayout {
    pub top_bar: Rect,
    pub transcript: Rect,
    pub status: Rect,
    pub hints: Option<Rect>,
    pub search: Option<Rect>,
    pub composer: Rect,
    pub footer: Rect,
}

pub(crate) fn shell_layout(
    area: Rect,
    hint_rows: u16,
    search_rows: u16,
    composer_rows: u16,
) -> ShellLayout {
    let hint_height = if hint_rows == 0 {
        0
    } else {
        hint_rows.min(4) + 2
    };
    let search_height = if search_rows == 0 {
        0
    } else {
        search_rows.min(4) + 2
    };
    let composer_height = (composer_rows + 2).clamp(MIN_COMPOSER_HEIGHT, MAX_COMPOSER_HEIGHT);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(TOP_BAR_HEIGHT),
            Constraint::Min(8),
            Constraint::Length(hint_height),
            Constraint::Length(search_height),
            Constraint::Length(composer_height),
            Constraint::Length(FOOTER_HEIGHT),
        ])
        .split(area);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(STATUS_WIDTH)])
        .split(vertical[1]);

    ShellLayout {
        top_bar: vertical[0],
        transcript: body[0],
        status: body[1],
        hints: if hint_height > 0 {
            Some(vertical[2])
        } else {
            None
        },
        search: if search_height > 0 {
            Some(vertical[3])
        } else {
            None
        },
        composer: vertical[4],
        footer: vertical[5],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_pane_uses_dedicated_row_even_when_hint_row_hidden() {
        let layout = shell_layout(Rect::new(0, 0, 100, 30), 0, 2, 1);
        let search = layout.search.expect("search rect");
        assert!(search.height > 0);
        assert!(layout.hints.is_none());
    }
}
