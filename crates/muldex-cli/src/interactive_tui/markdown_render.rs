use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use super::theme;

/// Lazily-loaded syntax definitions + theme used for code-block highlighting.
struct CodeHighlight {
    syntax_set: SyntaxSet,
    theme: Theme,
}

fn code_highlight() -> &'static CodeHighlight {
    static H: OnceLock<CodeHighlight> = OnceLock::new();
    H.get_or_init(|| {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme = ThemeSet::load_defaults()
            .themes
            .get("base16-ocean.dark")
            .cloned()
            .unwrap_or_else(|| ThemeSet::load_defaults().themes.values().next().cloned().unwrap());
        CodeHighlight { syntax_set, theme }
    })
}

fn rgb(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Highlight a code block, returning ratatui lines with per-token colors.
/// Returns `None` if the language can't be resolved or highlighting fails.
fn highlight_code_block(lang: &str, code: &str) -> Option<Vec<Line<'static>>> {
    let hl = code_highlight();
    let syntax = hl
        .syntax_set
        .find_syntax_by_token(lang)
        .unwrap_or_else(|| hl.syntax_set.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, &hl.theme);
    let mut out: Vec<Line<'static>> = Vec::new();
    for line in LinesWithEndings::from(code) {
        let ranges = highlighter.highlight_line(line, &hl.syntax_set).ok()?;
        let spans: Vec<Span<'static>> = ranges
            .into_iter()
            .map(|(s, text)| {
                Span::styled(text.to_string(), Style::default().fg(rgb(s.foreground)))
            })
            .collect();
        out.push(Line::from(spans));
    }
    Some(out)
}

/// Render markdown text into ratatui lines for display in the transcript.
pub(crate) fn render_markdown(text: &str) -> Vec<Line<'static>> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};

    let parser = Parser::new(text);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut list_depth: usize = 0;
    let mut in_code_block = false;
    let mut code_lang: String = String::new();
    let mut code_lines: Vec<String> = Vec::new();
    let mut link_url: Option<String> = None;
    let mut link_text: String = String::new();

    let mut push_current = |current: &mut Vec<Span<'static>>, lines: &mut Vec<Line<'static>>| {
        if !current.is_empty() {
            lines.push(Line::from(std::mem::take(current)));
        }
    };

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                push_current(&mut current, &mut lines);
                let size = level as usize;
                let prefix = "#".repeat(size.min(6));
                current.push(Span::styled(
                    format!("{prefix} "),
                    Style::default()
                        .fg(theme::markdown_heading())
                        .add_modifier(Modifier::BOLD),
                ));
            }
            Event::End(TagEnd::Heading(_)) => {
                push_current(&mut current, &mut lines);
                lines.push(Line::default());
            }
            Event::Start(Tag::Paragraph) => {
                push_current(&mut current, &mut lines);
            }
            Event::End(TagEnd::Paragraph) => {
                push_current(&mut current, &mut lines);
                lines.push(Line::default());
            }
            Event::Start(Tag::List(first)) => {
                list_depth += 1;
                if first.is_some() {
                    // ordered list marker handled per item
                }
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {
                push_current(&mut current, &mut lines);
                let indent = "  ".repeat(list_depth.saturating_sub(1));
                current.push(Span::styled(
                    format!("{indent}- "),
                    Style::default().fg(theme::markdown_bullet()),
                ));
            }
            Event::End(TagEnd::Item) => {
                push_current(&mut current, &mut lines);
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                push_current(&mut current, &mut lines);
                in_code_block = true;
                code_lines.clear();
                code_lang = match kind {
                    pulldown_cmark::CodeBlockKind::Indented => String::new(),
                    pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                };
            }
            Event::End(TagEnd::CodeBlock) => {
                let lang_label = if code_lang.is_empty() {
                    "code".to_string()
                } else {
                    code_lang.clone()
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        "── ",
                        Style::default().fg(theme::markdown_code_border()),
                    ),
                    Span::styled(
                        lang_label,
                        Style::default()
                            .fg(theme::markdown_code_border())
                            .add_modifier(Modifier::DIM),
                    ),
                    Span::styled(
                        " ",
                        Style::default().fg(theme::markdown_code_border()),
                    ),
                ]));
                let code_text: String = code_lines
                    .iter()
                    .map(|l| l.trim_end_matches('\n'))
                    .collect::<Vec<_>>()
                    .join("\n");
                if let Some(hl_lines) = highlight_code_block(&code_lang, &code_text) {
                    for hl in hl_lines {
                        let mut row: Vec<Span<'static>> = Vec::new();
                        row.push(Span::styled(
                            "│ ",
                            Style::default().fg(theme::markdown_code_border()),
                        ));
                        row.extend(hl.spans);
                        lines.push(Line::from(row));
                    }
                } else {
                    for code_line in code_text.lines() {
                        lines.push(Line::from(vec![
                            Span::styled(
                                "│ ",
                                Style::default().fg(theme::markdown_code_border()),
                            ),
                            Span::styled(
                                code_line.to_string(),
                                Style::default().fg(theme::markdown_code()),
                            ),
                        ]));
                    }
                }
                lines.push(Line::from(Span::styled(
                    "──",
                    Style::default().fg(theme::markdown_code_border()),
                )));
                lines.push(Line::default());
                in_code_block = false;
                code_lang.clear();
                code_lines.clear();
            }
            Event::Start(Tag::BlockQuote(_)) => {
                push_current(&mut current, &mut lines);
                current.push(Span::styled(
                    "> ",
                    Style::default()
                        .fg(theme::markdown_quote())
                        .add_modifier(Modifier::ITALIC),
                ));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                push_current(&mut current, &mut lines);
                lines.push(Line::default());
            }
            Event::Start(Tag::Emphasis) => {
                current.push(Span::styled(
                    "",
                    Style::default().add_modifier(Modifier::ITALIC),
                ));
            }
            Event::End(TagEnd::Emphasis) => {}
            Event::Start(Tag::Strong) => {
                current.push(Span::styled(
                    "",
                    Style::default().add_modifier(Modifier::BOLD),
                ));
            }
            Event::End(TagEnd::Strong) => {}
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_text.clear();
                link_url = Some(dest_url.to_string());
                current.push(Span::styled(
                    "[".to_string(),
                    Style::default().fg(theme::markdown_link()),
                ));
            }
            Event::End(TagEnd::Link) => {
                let url = link_url.take().unwrap_or_default();
                let text = std::mem::take(&mut link_text);
                // OSC 8 hyperlink: \x1b]8;;URL\x1b\\ TEXT \x1b]8;;\x1b\\
                // Terminals that support it render `text` as a clickable link;
                // unsupported terminals ignore the sequences.
                let hyperlink = format!("\x1b]8;;{url}\x1b\\{text}\x1b]8;;\x1b\\");
                current.push(Span::styled(
                    hyperlink,
                    Style::default()
                        .fg(theme::markdown_link())
                        .add_modifier(Modifier::UNDERLINED),
                ));
                current.push(Span::styled(
                    "](".to_string(),
                    Style::default().fg(theme::markdown_link()),
                ));
                current.push(Span::styled(
                    url,
                    Style::default()
                        .fg(theme::markdown_link())
                        .add_modifier(Modifier::DIM),
                ));
                current.push(Span::styled(
                    ")".to_string(),
                    Style::default().fg(theme::markdown_link()),
                ));
            }
            Event::Text(t) => {
                if in_code_block {
                    code_lines.push(t.to_string());
                } else if let Some(url) = link_url.as_ref() {
                    let _ = url;
                    link_text.push_str(t.as_ref());
                } else {
                    current.push(Span::raw(t.to_string()));
                }
            }
            Event::Code(c) => {
                if in_code_block {
                    code_lines.push(c.to_string());
                } else if link_url.is_some() {
                    link_text.push_str(c.as_ref());
                } else {
                    current.push(Span::styled(
                        format!("{c}"),
                        Style::default()
                            .fg(theme::markdown_inline_code())
                            .bg(Color::DarkGray),
                    ));
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    code_lines.push(String::new());
                } else {
                    push_current(&mut current, &mut lines);
                }
            }
            _ => {}
        }
    }

    push_current(&mut current, &mut lines);

    if lines.is_empty() {
        lines.push(Line::from(text.to_string()));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_plain_text_as_single_line() {
        let lines = render_markdown("hello world");
        let joined = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("hello world"));
    }

    #[test]
    fn renders_heading_with_marker() {
        let lines = render_markdown("# Title");
        let joined = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("# Title"));
    }

    #[test]
    fn renders_code_block_with_border() {
        let lines = render_markdown("```rust\nlet x = 1;\n```");
        let joined = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("rust"));
        assert!(joined.contains("let x = 1;"));
    }

    #[test]
    fn renders_link_with_visible_url() {
        let lines = render_markdown("[muldex](https://example.com)");
        let joined = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("muldex"));
        assert!(joined.contains("https://example.com"));
    }

    #[test]
    fn renders_link_as_osc8_hyperlink() {
        let lines = render_markdown("[muldex](https://example.com)");
        let joined = lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("\x1b]8;;https://example.com\x1b\\"));
        assert!(joined.contains("\x1b]8;;\x1b\\"));
    }

    #[test]
    fn highlights_rust_code_block_with_colors() {
        let lines = render_markdown("```rust\nlet x = 1;\n```");
        let has_color = lines.iter().any(|l| {
            l.spans.iter().any(|s| match s.style.fg {
                Some(Color::Rgb(_, _, _)) => true,
                _ => false,
            })
        });
        assert!(has_color, "expected at least one colored span from syntax highlighting");
    }
}
