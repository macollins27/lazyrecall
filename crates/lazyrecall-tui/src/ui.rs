//! Drawing layer. All rendering goes through `draw`.

use std::time::UNIX_EPOCH;

use lazyrecall_core::{Event as SessionEvent, EventKind};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Wrap,
};
use ratatui::Frame;

use crate::app::{App, Pane};
use crate::format::{display_project, format_relative, mtime, oneline, truncate_display};
use crate::theme;

pub fn draw(f: &mut Frame, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(1), // status
            Constraint::Length(1), // help
        ])
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
    draw_help(f, outer[2], app);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let pending = app.stats.total.saturating_sub(app.stats.summarized);
    let (status_text, status_color) = if !app.api_key_set {
        ("no ANTHROPIC_API_KEY", theme::STATUS_WARN)
    } else if pending == 0 {
        ("summarizer idle", theme::STATUS_OK)
    } else {
        ("summarizer working", theme::STATUS_WORK)
    };
    let line = Line::from(vec![
        Span::styled(" ", theme::dim()),
        Span::styled(
            format!("{}", app.stats.total),
            Style::default().fg(Color::White),
        ),
        Span::styled(" sessions · ", theme::dim()),
        Span::styled(
            format!("{}", app.stats.summarized),
            Style::default().fg(theme::STATUS_OK),
        ),
        Span::styled(" summarized · ", theme::dim()),
        Span::styled(
            format!("{pending}"),
            Style::default().fg(theme::STATUS_WARN),
        ),
        Span::styled(" pending · ", theme::dim()),
        Span::styled(status_text, Style::default().fg(status_color)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::raw(" "));
    let pairs: &[(&str, &str)] = match app.focus {
        Pane::Preview => &[
            ("q", "quit"),
            ("1/2/3", "pane"),
            ("h/l", "switch"),
            ("j/k", "scroll"),
            ("PgUp/Dn", "page"),
            ("g/G", "top/bot"),
            ("?", "help"),
        ],
        _ => &[
            ("q", "quit"),
            ("1/2/3", "pane"),
            ("Tab", "next"),
            ("j/k", "move"),
            ("Enter", "resume"),
            ("?", "help"),
        ],
    };
    for (i, (key, label)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", theme::dim()));
        }
        spans.push(Span::styled(*key, Style::default().fg(theme::HELP_KEY)));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*label, Style::default().fg(theme::HELP_LABEL)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_projects(f: &mut Frame, area: Rect, app: &mut App) {
    let total = app.projects.len();
    let selected = app.project_state.selected().unwrap_or(0);
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
    let block = pane_block("projects", 1, app.focus == Pane::Projects, selected, total);
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected());
    f.render_stateful_widget(list, area, &mut app.project_state);
    render_list_scrollbar(f, area, total, selected);
}

fn draw_sessions(f: &mut Frame, area: Rect, app: &mut App) {
    let total = app.sessions.len();
    let selected = app.session_state.selected().unwrap_or(0);
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
    let block = pane_block("sessions", 2, app.focus == Pane::Sessions, selected, total);
    let list = List::new(items)
        .block(block)
        .highlight_style(theme::selected());
    f.render_stateful_widget(list, area, &mut app.session_state);
    render_list_scrollbar(f, area, total, selected);
}

fn draw_preview(f: &mut Frame, area: Rect, app: &mut App) {
    let meta = app.current_session_metadata().cloned();
    let events = app.current_events().cloned();
    let scroll = app.preview_scroll;

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
        lines.push(Line::from(Span::styled("── conversation ──", theme::dim())));
        lines.push(Line::from(""));
    }

    if let Some(events) = &events {
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
    }

    let total_lines = lines.len();
    app.preview_total_lines = total_lines as u16;

    // Manual scroll via slicing rather than `Paragraph::scroll((y, x))` —
    // the latter combined with `Wrap { trim: false }` overruns the pane's
    // buffer in ratatui 0.28, causing wrapped text to bleed into adjacent
    // panes (e.g. preview text appearing as a prefix in the projects list).
    let visible: Vec<Line> = lines.into_iter().skip(scroll as usize).collect();
    let block = pane_block_simple("preview", 3, app.focus == Pane::Preview);
    let p = Paragraph::new(visible)
        .wrap(Wrap { trim: false })
        .block(block);
    f.render_widget(p, area);

    render_paragraph_scrollbar(f, area, total_lines, scroll);
}

fn render_list_scrollbar(f: &mut Frame, area: Rect, total: usize, selected: usize) {
    if total <= 1 || area.height < 3 {
        return;
    }
    let mut state = ScrollbarState::new(total).position(selected);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .symbols(symbols::scrollbar::VERTICAL)
        .track_style(Style::default().fg(theme::SCROLLBAR))
        .thumb_style(Style::default().fg(theme::SCROLLBAR_THUMB))
        .begin_symbol(None)
        .end_symbol(None);
    // Inset by one row top and bottom so the scrollbar sits inside the border
    // corners instead of overpainting them.
    let track = Rect {
        x: area.x + area.width.saturating_sub(1),
        y: area.y + 1,
        width: 1,
        height: area.height.saturating_sub(2),
    };
    f.render_stateful_widget(scrollbar, track, &mut state);
}

fn render_paragraph_scrollbar(f: &mut Frame, area: Rect, total_lines: usize, scroll: u16) {
    if total_lines <= 1 || area.height < 3 {
        return;
    }
    let mut state = ScrollbarState::new(total_lines).position(scroll as usize);
    let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
        .symbols(symbols::scrollbar::VERTICAL)
        .track_style(Style::default().fg(theme::SCROLLBAR))
        .thumb_style(Style::default().fg(theme::SCROLLBAR_THUMB))
        .begin_symbol(None)
        .end_symbol(None);
    let track = Rect {
        x: area.x + area.width.saturating_sub(1),
        y: area.y + 1,
        width: 1,
        height: area.height.saturating_sub(2),
    };
    f.render_stateful_widget(scrollbar, track, &mut state);
}

fn pane_block(
    title: &str,
    number: u8,
    focused: bool,
    selected: usize,
    total: usize,
) -> Block<'static> {
    let mut spans = vec![
        Span::raw(" "),
        Span::styled(
            format!("[{number}]"),
            Style::default().fg(theme::PANE_NUMBER),
        ),
        Span::raw(" "),
        Span::styled(title.to_string(), title_style(focused)),
    ];
    if total > 0 {
        spans.push(Span::styled(
            format!(" ({}/{})", selected + 1, total),
            theme::dim(),
        ));
    }
    spans.push(Span::raw(" "));

    Block::default()
        .title(Line::from(spans))
        .borders(Borders::ALL)
        .border_style(border_style(focused))
}

fn pane_block_simple(title: &str, number: u8, focused: bool) -> Block<'static> {
    let spans = vec![
        Span::raw(" "),
        Span::styled(
            format!("[{number}]"),
            Style::default().fg(theme::PANE_NUMBER),
        ),
        Span::raw(" "),
        Span::styled(title.to_string(), title_style(focused)),
        Span::raw(" "),
    ];
    Block::default()
        .title(Line::from(spans))
        .borders(Borders::ALL)
        .border_style(border_style(focused))
}

fn title_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(theme::BORDER_FOCUSED)
            .add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::BORDER)
            .add_modifier(ratatui::style::Modifier::BOLD)
    }
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(theme::BORDER_FOCUSED)
    } else {
        Style::default().fg(theme::BORDER)
    }
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
    let bold = ratatui::style::Modifier::BOLD;
    match kind {
        EventKind::UserText(_) => Style::default().fg(theme::USER).add_modifier(bold),
        EventKind::UserToolResult { .. } => Style::default().fg(theme::RESULT).add_modifier(bold),
        EventKind::AssistantText(_) => Style::default().fg(theme::ASSISTANT).add_modifier(bold),
        EventKind::AssistantToolUse { .. } => Style::default().fg(theme::TOOL).add_modifier(bold),
        EventKind::System(_) => Style::default().fg(theme::SYSTEM).add_modifier(bold),
    }
}
