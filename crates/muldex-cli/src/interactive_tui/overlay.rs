#[derive(Debug, Clone, Default)]
pub(crate) struct OverlayState {
    pub visible: bool,
    pub title: String,
    pub lines: Vec<String>,
    pub scroll: usize,
    pub kind: OverlayKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum OverlayKind {
    #[default]
    Pager,
    Approval,
}

impl OverlayState {
    pub(crate) fn show_pager(title: String, lines: Vec<String>) -> Self {
        Self {
            visible: true,
            title,
            lines,
            scroll: 0,
            kind: OverlayKind::Pager,
        }
    }

    pub(crate) fn show_approval(title: String, summary: String) -> Self {
        Self {
            visible: true,
            title,
            lines: vec![
                summary,
                String::new(),
                "Approve: a / Enter   Deny: d / Esc   Cancel: Ctrl+C".to_string(),
            ],
            scroll: 0,
            kind: OverlayKind::Approval,
        }
    }

    pub(crate) fn hide(&mut self) {
        self.visible = false;
        self.lines.clear();
        self.scroll = 0;
    }

    pub(crate) fn scroll_up(&mut self, rows: usize) {
        self.scroll = self.scroll.saturating_sub(rows.max(1));
    }

    pub(crate) fn scroll_down(&mut self, rows: usize) {
        self.scroll = self.scroll.saturating_add(rows.max(1));
    }

    pub(crate) fn page_up(&mut self, page: usize) {
        self.scroll = self.scroll.saturating_sub(page.max(1));
    }

    pub(crate) fn page_down(&mut self, page: usize) {
        self.scroll = self.scroll.saturating_add(page.max(1));
    }

    pub(crate) fn jump_top(&mut self) {
        self.scroll = 0;
    }

    pub(crate) fn jump_bottom(&mut self) {
        self.scroll = usize::MAX / 2;
    }

    pub(crate) fn visible_range(&self, height: usize) -> std::ops::Range<usize> {
        let height = height.max(1);
        let start = self.scroll.min(self.lines.len());
        let end = (start + height).min(self.lines.len());
        start..end
    }
}
