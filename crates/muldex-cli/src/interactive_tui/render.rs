use ratatui::layout::Position;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, Paragraph, Wrap};

use super::frames::FrameStyle;
use super::layout;
use super::theme;
use super::view_model::{
    ComposerMode, HistorySearchViewModel, OverlayViewModel, RateLimitViewModel, ShellViewModel,
    SlashHintViewModel, TranscriptItemViewModel, UsageViewModel,
};

pub(crate) fn render_shell(frame: &mut ratatui::Frame<'_>, view_model: &ShellViewModel) {
    let composer_rows = composer_visual_rows(&view_model.composer.text, frame.area().width);
    let layout = layout::shell_layout(
        frame.area(),
        view_model.slash_hints.len() as u16,
        view_model
            .search
            .as_ref()
            .map(|search| search.lines.len() as u16)
            .unwrap_or(0),
        composer_rows,
    );

    render_top_bar(frame, layout.top_bar, view_model);
    render_transcript(frame, layout.transcript, view_model);
    render_status(frame, layout.status, view_model);
    if let Some(area) = layout.hints {
        render_hints(frame, area, &view_model.slash_hints);
    }
    if let Some(area) = layout.search {
        if let Some(search) = &view_model.search {
            render_search(frame, area, search);
        }
    }
    render_composer(frame, layout.composer, view_model);
    render_footer(frame, layout.footer, view_model);
    if view_model.overlay.visible {
        render_overlay(frame, frame.area(), &view_model.overlay);
    }
}

fn render_top_bar(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    view_model: &ShellViewModel,
) {
    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", view_model.top_bar.product_name),
            theme::top_bar(),
        ),
        Span::raw("  "),
        Span::raw(&view_model.top_bar.session_summary),
        Span::raw("  •  "),
        Span::raw(&view_model.top_bar.phase),
        Span::raw("  •  "),
        Span::raw(&view_model.top_bar.model),
        Span::raw("  •  "),
        Span::raw(&view_model.top_bar.approval_mode),
        Span::raw("  •  "),
        Span::raw(&view_model.top_bar.cycle_summary),
    ])
    .style(theme::top_bar());
    frame.render_widget(Paragraph::new(line), area);
}

fn render_transcript(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    view_model: &ShellViewModel,
) {
    let block = FrameStyle::Codex.titled_block(theme::TRANSCRIPT_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if view_model.transcript_items.is_empty() {
        frame.render_widget(
            Paragraph::new(theme::EMPTY_TRANSCRIPT)
                .style(theme::placeholder())
                .wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }

    let items = transcript_tail_for_area(&view_model.transcript_items, inner)
        .iter()
        .map(transcript_item)
        .collect::<Vec<_>>();
    frame.render_widget(List::new(items), inner);
}

fn transcript_tail_for_area(
    items: &[TranscriptItemViewModel],
    area: ratatui::layout::Rect,
) -> Vec<TranscriptItemViewModel> {
    if items.is_empty() || area.height == 0 || area.width == 0 {
        return items.to_vec();
    }

    let width = area.width.max(1) as usize;
    let mut kept = Vec::new();
    let mut used_rows = 0usize;

    for item in items.iter().rev() {
        let rows = transcript_item_rows(item, width);
        if !kept.is_empty() && used_rows + rows > area.height as usize {
            break;
        }
        kept.push(item.clone());
        used_rows += rows;
        if used_rows >= area.height as usize {
            break;
        }
    }

    kept.reverse();
    kept
}

fn transcript_item_rows(item: &TranscriptItemViewModel, width: usize) -> usize {
    let role_rows = wrapped_line_count(&format!("[{}]", item.role_label), width);
    let content_rows = item
        .content
        .split('\n')
        .map(|line| wrapped_line_count(line, width))
        .sum::<usize>()
        .max(1);
    role_rows + content_rows + 1
}

fn wrapped_line_count(text: &str, width: usize) -> usize {
    let display_width = text.chars().count().max(1);
    display_width.div_ceil(width.max(1))
}

fn render_status(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    view_model: &ShellViewModel,
) {
    let block = FrameStyle::Codex.titled_block(theme::STATUS_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let provider = if view_model.status_panel.provider_summary.trim().is_empty() {
        theme::PROVIDER_NOT_CONFIGURED.to_string()
    } else {
        view_model.status_panel.provider_summary.clone()
    };
    let usage = &view_model.status_panel.usage;
    let usage_line = if usage.total_tokens > 0 {
        format!(
            "in={} out={} cached={} total={}",
            usage.input_tokens, usage.output_tokens, usage.cached_input_tokens, usage.total_tokens
        )
    } else {
        "—".to_string()
    };
    let rate_limit = &view_model.status_panel.rate_limit;
    let rate_limit_line = if rate_limit.remaining_requests.is_some()
        || rate_limit.remaining_tokens.is_some()
    {
        format!(
            "req {}/{} · tok {}/{}",
            rate_limit.remaining_requests.unwrap_or(0),
            rate_limit.limit_requests.unwrap_or(0),
            rate_limit.remaining_tokens.unwrap_or(0),
            rate_limit.limit_tokens.unwrap_or(0),
        )
    } else {
        "—".to_string()
    };
    let lines = vec![
        labeled_line("Phase", &view_model.status_panel.phase),
        labeled_line("Objective", &view_model.status_panel.objective),
        labeled_line("Last", &view_model.status_panel.last_outcome),
        labeled_line(
            "Pending approval",
            if view_model.status_panel.pending_approval {
                "yes"
            } else {
                "no"
            },
        ),
        labeled_line(
            "Compact count",
            &view_model.status_panel.compact_count.to_string(),
        ),
        labeled_line(
            "Resume count",
            &view_model.status_panel.resume_count.to_string(),
        ),
        labeled_line("Provider", &provider),
        labeled_line("Model", &view_model.status_panel.model_summary),
        labeled_line("Tokens", &usage_line),
        labeled_line("Rate limit", &rate_limit_line),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_hints(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    hints: &[SlashHintViewModel],
) {
    let block = FrameStyle::Codex.titled_block(theme::SLASH_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let items = hints
        .iter()
        .map(|hint| {
            let style = if hint.selected {
                theme::hint_selected()
            } else {
                theme::hint_normal()
            };
            ListItem::new(Line::from(vec![
                Span::raw(if hint.selected { "> " } else { "  " }),
                Span::styled(&hint.command, style),
                Span::raw(" - "),
                Span::raw(&hint.summary),
            ]))
        })
        .collect::<Vec<_>>();
    frame.render_widget(List::new(items), inner);
}

fn render_search(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    search: &HistorySearchViewModel,
) {
    let block = FrameStyle::Codex.titled_block(theme::SEARCH_TITLE);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = search
        .lines
        .iter()
        .cloned()
        .map(Line::from)
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_composer(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    view_model: &ShellViewModel,
) {
    let title = match view_model.composer.mode {
        ComposerMode::Normal => ratatui::text::Line::from(vec![
            ratatui::text::Span::styled(
                format!("{}  ", theme::COMPOSER_TITLE),
                theme::section_title(),
            ),
            ratatui::text::Span::styled("-- NORMAL --", theme::vim_normal_mode()),
        ]),
        ComposerMode::Insert => ratatui::text::Line::from(ratatui::text::Span::styled(
            theme::COMPOSER_TITLE,
            theme::section_title(),
        )),
    };
    let block = FrameStyle::Codex.titled_block_line(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = composer_lines(&view_model.composer.text);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );

    let (cursor_row, cursor_col) = composer_cursor_offset(
        &view_model.composer.text,
        view_model.composer.cursor_line,
        view_model.composer.cursor_column,
        inner.width,
    );
    let cursor_x = inner.x.saturating_add(cursor_col);
    let cursor_y = inner.y.saturating_add(cursor_row);
    if cursor_x < inner.right() && cursor_y < inner.bottom() {
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }
}

fn render_footer(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    view_model: &ShellViewModel,
) {
    if area.height == 0 {
        return;
    }
    let spinner = {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        ["|", "/", "-", "\\"][(now_ms / 200) as usize % 4]
    };
    let badge = match view_model.footer.status_badge.as_str() {
        "thinking" => (format!(" {spinner} thinking"), theme::footer_badge_thinking()),
        "busy" => (format!(" {spinner} busy"), theme::footer_badge_busy()),
        "approval" => (format!(" {spinner} approval"), theme::footer_badge_thinking()),
        _ => (" ● idle".to_string(), theme::footer_badge_idle()),
    };
    let hint = if view_model.footer.hint.is_empty() {
        "enter: send · esc: cancel · ctrl-c: exit · ?: commands".to_string()
    } else {
        view_model.footer.hint.clone()
    };
    if let Some(notification) = super::notifications::current_notification() {
        let right_width =
            notification.chars().count().min(area.width.saturating_sub(12) as usize);
        let right = notification
            .chars()
            .rev()
            .take(right_width)
            .collect::<Vec<_>>()
            .iter()
            .rev()
            .collect::<String>();
        let mut spans = vec![Span::styled(badge.0, badge.1)];
        push_footer_token_summary(&mut spans, &view_model.footer.token_summary);
        spans.push(Span::raw("   "));
        spans.push(Span::styled(right, super::notifications::notification_style()));
        let line = Line::from(spans);
        frame.render_widget(Paragraph::new(line).style(theme::footer_style()), area);
        return;
    }
    let right_width = hint.chars().count().min(area.width.saturating_sub(12) as usize);
    let right = &hint.chars().rev().take(right_width).collect::<Vec<_>>().iter().rev().collect::<String>();
    let mut spans = vec![Span::styled(badge.0, badge.1)];
    push_footer_token_summary(&mut spans, &view_model.footer.token_summary);
    spans.push(Span::raw("   "));
    spans.push(Span::styled(right.clone(), theme::footer_hint()));
    let line = Line::from(spans);
    frame.render_widget(
        Paragraph::new(line).style(theme::footer_style()),
        area,
    );
}

fn push_footer_token_summary(spans: &mut Vec<Span<'static>>, token_summary: &str) {
    if !token_summary.is_empty() {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            token_summary.to_string(),
            theme::footer_token_summary(),
        ));
    }
}

fn render_overlay(
    frame: &mut ratatui::Frame<'_>,
    area: ratatui::layout::Rect,
    overlay: &OverlayViewModel,
) {
    use ratatui::layout::{Constraint, Direction, Layout};
    use ratatui::widgets::Block;

    let is_approval = overlay.kind == "approval";
    let popup_height = area.height.saturating_sub(4).max(3).min(area.height);
    let popup_width = area.width.saturating_sub(8).max(20).min(area.width);
    let popup_area = centered_rect(popup_width, popup_height, area);

    let block = if is_approval {
        FrameStyle::Codex
            .titled_block(" Approval Request ")
            .style(theme::approval_block())
    } else {
        let title = format!(" {} ", overlay.title);
        FrameStyle::Codex
            .titled_block(&title)
            .style(theme::overlay_block())
    };
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let content_area = if is_approval {
        inner
    } else {
        ratatui::layout::Rect {
            y: inner.y,
            height: inner.height.saturating_sub(1),
            ..inner
        }
    };

    let visible: Vec<Line<'static>> = overlay
        .lines
        .iter()
        .skip(overlay.scroll)
        .take(content_area.height as usize)
        .map(|line| Line::from(line.clone()))
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(visible)).wrap(Wrap { trim: false }),
        content_area,
    );

    if is_approval {
        let hint_area = ratatui::layout::Rect {
            x: inner.x,
            y: inner.y + inner.height.saturating_sub(1),
            width: inner.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                "a/Enter approve  d/Esc deny  Ctrl+C cancel",
                theme::approval_hint(),
            )])),
            hint_area,
        );
    } else {
        let footer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(inner);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!(" {}/{} ", overlay.scroll + 1, overlay.lines.len().max(1)),
                    theme::overlay_hint(),
                ),
                Span::styled(
                    " j/k scroll  g/G top/bottom  q close ",
                    theme::overlay_hint(),
                ),
            ])),
            footer[1],
        );
    }
}

fn centered_rect(width: u16, height: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    ratatui::layout::Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}

fn labeled_line(label: &str, value: &str) -> Line<'static> {    Line::from(vec![
        Span::styled(format!("{label}: "), theme::status_label()),
        Span::raw(value.to_string()),
    ])
}

fn transcript_item(item: &TranscriptItemViewModel) -> ListItem<'static> {
    let role_line = Line::from(vec![Span::styled(
        format!("[{}]", item.role_label),
        theme::transcript_role(&item.role_label),
    )]);

    let content_lines: Vec<Line<'static>> = if item.role_label == "ASSISTANT" {
        super::markdown_render::render_markdown(&item.content)
    } else {
        item.content
            .split('\n')
            .map(|line| Line::from(line.to_string()))
            .collect()
    };

    let mut rows = Vec::with_capacity(content_lines.len() + 2);
    rows.push(role_line);
    rows.extend(content_lines);
    rows.push(Line::default());
    ListItem::new(rows)
}

fn composer_lines(text: &str) -> Vec<Line<'static>> {
    let normalized = if text.is_empty() {
        vec![String::new()]
    } else {
        text.split('\n').map(str::to_string).collect::<Vec<_>>()
    };
    normalized
        .into_iter()
        .enumerate()
        .map(|(index, line)| {
            let prefix = if index == 0 { "> " } else { "  " };
            Line::from(vec![
                Span::styled(prefix.to_string(), theme::composer_prefix()),
                Span::raw(line),
            ])
        })
        .collect()
}

fn composer_visual_rows(text: &str, width: u16) -> u16 {
    let width = width.saturating_sub(2).max(1) as usize;
    let lines = if text.is_empty() {
        vec![String::new()]
    } else {
        text.split('\n').map(str::to_string).collect::<Vec<_>>()
    };

    lines
        .iter()
        .map(|line| prefixed_visual_rows(line.chars().count(), width) as u16)
        .sum::<u16>()
        .max(1)
}

fn composer_cursor_offset(
    text: &str,
    cursor_line: usize,
    cursor_column: usize,
    width: u16,
) -> (u16, u16) {
    let width = width.max(1) as usize;
    let lines = if text.is_empty() {
        vec![String::new()]
    } else {
        text.split('\n').map(str::to_string).collect::<Vec<_>>()
    };

    let mut row = 0usize;
    for line in lines.iter().take(cursor_line) {
        row += prefixed_visual_rows(line.chars().count(), width);
    }

    let prefixed_column = 2 + cursor_column;
    row += prefixed_column / width;
    let col = prefixed_column % width;
    (row as u16, col as u16)
}

fn prefixed_visual_rows(content_width: usize, width: usize) -> usize {
    let total_columns = 2 + content_width;
    total_columns.max(1).div_ceil(width.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interactive_tui::render_to_buffer;
    use crate::interactive_tui::view_model::{
        ComposerViewModel, FooterViewModel, HistorySearchViewModel, ShellViewModel, SlashHintViewModel,
        StatusPanelViewModel, TopBarViewModel, TranscriptItemViewModel,
    };

    fn sample_view_model() -> ShellViewModel {
        ShellViewModel {
            top_bar: TopBarViewModel {
                product_name: "muldex".to_string(),
                session_summary: "session: demo-session".to_string(),
                phase: "phase: Running".to_string(),
                model: "model: gpt-5.4".to_string(),
                approval_mode: "approval: on-request".to_string(),
                cycle_summary: "cycle: 3".to_string(),
            },
            transcript_items: vec![
                TranscriptItemViewModel {
                    role_label: "SYSTEM".to_string(),
                    content: "shell created".to_string(),
                },
                TranscriptItemViewModel {
                    role_label: "USER".to_string(),
                    content: "hello world".to_string(),
                },
            ],
            status_panel: StatusPanelViewModel {
                phase: "Running".to_string(),
                objective: "ship demo".to_string(),
                last_outcome: "interactive prompt: hello".to_string(),
                pending_approval: true,
                busy: false,
                compact_count: 1,
                resume_count: 2,
                provider_summary: "llm-router / gpt-5.4".to_string(),
                model_summary: "gpt-5.4".to_string(),
                usage: UsageViewModel::default(),
                rate_limit: RateLimitViewModel::default(),
            },
            composer: ComposerViewModel {
                text: "/model".to_string(),
                cursor_line: 0,
                cursor_column: 6,
                mode: ComposerMode::Insert,
            },
            slash_hints: vec![SlashHintViewModel {
                command: "/model".to_string(),
                summary: "show or set active model".to_string(),
                selected: true,
            }],
            search: Some(HistorySearchViewModel {
                lines: vec![
                    "reverse search active: mo".to_string(),
                    "matches: 1".to_string(),
                ],
            }),
            footer: FooterViewModel {
                status_badge: "idle".to_string(),
                hint: String::new(),
                token_summary: String::new(),
            },
            overlay: OverlayViewModel {
                visible: false,
                title: String::new(),
                lines: Vec::new(),
                scroll: 0,
                kind: "pager".to_string(),
            },
        }
    }

    fn buffer_lines(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
        buffer
            .content
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    fn joined_buffer(buffer: &ratatui::buffer::Buffer) -> String {
        buffer_lines(buffer).join("\n")
    }

    #[test]
    fn renderer_frame_contains_top_titles_and_section_labels() {
        let buffer = render_to_buffer(&sample_view_model(), 100, 30).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("muldex"));
        assert!(rendered.contains("Transcript"));
        assert!(rendered.contains("Status"));
        assert!(rendered.contains("Composer"));
    }

    #[test]
    fn renderer_transcript_pane_renders_role_markers() {
        let buffer = render_to_buffer(&sample_view_model(), 100, 30).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("[SYSTEM]"));
        assert!(rendered.contains("[USER]"));
    }

    #[test]
    fn renderer_status_pane_renders_approval_and_model_labels() {
        let buffer = render_to_buffer(&sample_view_model(), 100, 30).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("Pending approval: yes"));
        assert!(rendered.contains("Model: gpt-5.4"));
    }

    #[test]
    fn renderer_composer_renders_prompt_content_and_cursor_line_prefix() {
        let buffer = render_to_buffer(&sample_view_model(), 100, 30).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("> /model"));
    }

    #[test]
    fn renderer_composer_shows_normal_mode_indicator_when_vim_normal() {
        let mut view_model = sample_view_model();
        view_model.composer.mode = ComposerMode::Normal;
        let buffer = render_to_buffer(&view_model, 100, 30).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("-- NORMAL --"));
    }

    #[test]
    fn renderer_empty_transcript_renders_placeholder() {
        let mut view_model = sample_view_model();
        view_model.transcript_items.clear();
        let buffer = render_to_buffer(&view_model, 100, 30).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("No messages yet"));
    }

    #[test]
    fn renderer_missing_provider_renders_explicit_not_configured_state() {
        let mut view_model = sample_view_model();
        view_model.status_panel.provider_summary.clear();
        let buffer = render_to_buffer(&view_model, 100, 30).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("provider not configured"));
    }

    #[test]
    fn renderer_long_session_and_objective_stay_within_layout_width() {
        let mut view_model = sample_view_model();
        view_model.top_bar.session_summary =
            "session: interactive-session-abcdefghijklmnopqrstuvwxyz-1234567890".to_string();
        view_model.status_panel.objective = "objective objective objective objective objective objective objective objective objective objective".to_string();
        let buffer = render_to_buffer(&view_model, 80, 24).expect("render buffer");
        assert!(
            buffer_lines(&buffer)
                .iter()
                .all(|line| line.chars().count() <= 80)
        );
    }

    #[test]
    fn renderer_keeps_newest_transcript_items_visible() {
        let mut view_model = sample_view_model();
        view_model.transcript_items = (0..12)
            .map(|index| TranscriptItemViewModel {
                role_label: "USER".to_string(),
                content: format!("message-{index}"),
            })
            .collect();
        let buffer = render_to_buffer(&view_model, 80, 16).expect("render buffer");
        let rendered = joined_buffer(&buffer);
        assert!(rendered.contains("message-11"));
        assert!(!rendered.contains("message-0"));
    }

    #[test]
    fn composer_cursor_offset_accounts_for_soft_wrap() {
        let (row, col) = composer_cursor_offset("abcdefghij", 0, 10, 6);
        assert_eq!((row, col), (2, 0));

        let (row, col) = composer_cursor_offset("hello\nworld", 1, 5, 8);
        assert_eq!((row, col), (1, 7));
    }

    #[test]
    fn composer_visual_rows_grows_for_multiline_and_wrapped_input() {
        assert_eq!(composer_visual_rows("", 20), 1);
        assert_eq!(composer_visual_rows("hello", 20), 1);
        assert!(composer_visual_rows("hello\nworld\nagain", 20) >= 3);
        assert!(composer_visual_rows("abcdefghij", 6) >= 3);
    }
}
