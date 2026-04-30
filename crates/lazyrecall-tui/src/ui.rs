//! Drawing layer. All rendering goes through `draw`.

use std::time::UNIX_EPOCH;

use lazyrecall_core::{Event as SessionEvent, EventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Pane};
use crate::format::{display_project, format_relative, mtime, oneline, truncate_display};
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(f.area());

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ])
        .split(outer[0]);

    draw_projects(f, panes[0], app);
    draw_sessions(f, panes[1], app);
    draw_preview(f, panes[2], app);
    draw_status(f, outer[1], app);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let pending = app.stats.total.saturating_sub(app.stats.summarized);
    let (status_text, status_color) = if !app.api_key_set {
        ("no ANTHROPIC_API_KEY", theme::WARN)
    } else if pending == 0 {
        ("summarizer idle", theme::OK)
    } else {
        ("summarizer working", theme::ACCENT)
    };
    let line = Line::from(vec![
        Span::raw(format!(
            " {} sessions · {} summarized · {} pending · ",
            app.stats.total, app.stats.summarized, pending
        )),
        Span::styled(status_text, Style::default().fg(status_color)),
    ]);
    let p = Paragraph::new(line).style(theme::dim());
    f.render_widget(p, area);
}

fn draw_projects(f: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            let name = display_project(p);
            let count = format!("({})", p.session_count);
            let when = p.latest_mtime_unix.map(format_relative).unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::raw(name),
                Span::raw(" "),
                Span::styled(count, theme::dim()),
                Span::raw("  "),
                Span::styled(when, theme::dim()),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(border(" projects ", app.focus == Pane::Projects))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut app.project_state);
}

fn draw_sessions(f: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .sessions
        .iter()
        .map(|p| {
            let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
            let when = mtime(p)
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| format_relative(d.as_secs() as i64))
                .unwrap_or_else(|| "?".to_string());
            let line = match app.summary_cache.get(id) {
                Some(s) => Line::from(vec![
                    Span::raw(truncate_display(&oneline(s), 60)),
                    Span::raw("  "),
                    Span::styled(when, theme::dim()),
                ]),
                None => Line::from(Span::styled(when, theme::dim())),
            };
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(border(" sessions ", app.focus == Pane::Sessions))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut app.session_state);
}

fn draw_preview(f: &mut Frame, area: Rect, app: &mut App) {
    let meta = app.current_session_metadata().cloned();
    let recent = app.current_recent_events().cloned();

    let mut lines: Vec<Line> = Vec::new();

    if let Some(meta) = &meta {
        lines.push(Line::from(format!("id:       {}", meta.id)));
        if let Some(cwd) = &meta.cwd {
            lines.push(Line::from(format!("cwd:      {cwd}")));
        }
        lines.push(Line::from(format!("messages: {}", meta.message_count)));
        lines.push(Line::from(format!(
            "mtime:    {}",
            format_relative(meta.last_modified_unix)
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("── recent ──", theme::dim())));
        lines.push(Line::from(""));
    }

    if let Some(events) = &recent {
        for ev in events {
            let (label, body) = format_event(ev);
            let label_style = label_style_for(&ev.kind);
            lines.push(Line::from(vec![
                Span::styled(label, label_style),
                Span::raw(body),
            ]));
            lines.push(Line::from(""));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from("(no session selected)"));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "q: quit   j/k: nav   tab: focus   enter: resume",
            theme::dim(),
        )));
    }

    let p = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(border(" preview ", app.focus == Pane::Preview));
    f.render_widget(p, area);
}

fn format_event(ev: &SessionEvent) -> (&'static str, String) {
    match &ev.kind {
        EventKind::UserText(t) => ("[user]   ", oneline(t)),
        EventKind::UserToolResult { content, .. } => ("[result] ", oneline(content)),
        EventKind::AssistantText(t) => ("[claude] ", oneline(t)),
        EventKind::AssistantToolUse { name, input } => {
            ("[tool]   ", format!("{name}({})", oneline(input)))
        }
        EventKind::System(t) => ("[system] ", oneline(t)),
    }
}

fn label_style_for(kind: &EventKind) -> Style {
    let bold = Modifier::BOLD;
    match kind {
        EventKind::UserText(_) => Style::default().fg(theme::USER).add_modifier(bold),
        EventKind::UserToolResult { .. } => Style::default().fg(theme::RESULT).add_modifier(bold),
        EventKind::AssistantText(_) => Style::default().fg(theme::ASSISTANT).add_modifier(bold),
        EventKind::AssistantToolUse { .. } => Style::default().fg(theme::TOOL).add_modifier(bold),
        EventKind::System(_) => Style::default().fg(theme::SYSTEM).add_modifier(bold),
    }
}

fn border(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        theme::focused_border()
    } else {
        Style::default()
    };
    Block::default()
        .title(Span::styled(title, style))
        .borders(Borders::ALL)
        .border_style(style)
}
