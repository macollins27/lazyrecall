use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use recall_core::{discovery, parser, EventKind, Project};
use recall_core::Event as SessionEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    Projects,
    Sessions,
    Preview,
}

struct App {
    projects: Vec<Project>,
    project_state: ListState,
    sessions: Vec<PathBuf>,
    session_state: ListState,
    metadata_cache: HashMap<PathBuf, parser::SessionMetadata>,
    recent_cache: HashMap<PathBuf, Vec<SessionEvent>>,
    focus: Pane,
    resume_request: Option<String>,
    last_loaded_project_idx: Option<usize>,
}

impl App {
    fn new(projects: Vec<Project>) -> Self {
        let mut project_state = ListState::default();
        if !projects.is_empty() {
            project_state.select(Some(0));
        }
        Self {
            projects,
            project_state,
            sessions: Vec::new(),
            session_state: ListState::default(),
            metadata_cache: HashMap::new(),
            recent_cache: HashMap::new(),
            focus: Pane::Projects,
            resume_request: None,
            last_loaded_project_idx: None,
        }
    }

    fn refresh_sessions(&mut self) {
        let Some(idx) = self.project_state.selected() else {
            return;
        };
        if self.last_loaded_project_idx == Some(idx) {
            return;
        }
        let project = self.projects[idx].clone();
        self.sessions = discovery::list_sessions(&project).unwrap_or_default();
        self.sessions.sort_by(|a, b| {
            mtime(b).cmp(&mtime(a))
        });
        self.session_state = ListState::default();
        if !self.sessions.is_empty() {
            self.session_state.select(Some(0));
        }
        self.last_loaded_project_idx = Some(idx);
    }

    fn current_session(&self) -> Option<&PathBuf> {
        self.session_state
            .selected()
            .and_then(|i| self.sessions.get(i))
    }

    fn current_session_metadata(&mut self) -> Option<&parser::SessionMetadata> {
        let path = self.current_session()?.clone();
        if !self.metadata_cache.contains_key(&path) {
            if let Ok(meta) = parser::parse_metadata(&path) {
                self.metadata_cache.insert(path.clone(), meta);
            }
        }
        self.metadata_cache.get(&path)
    }

    fn current_recent_events(&mut self) -> Option<&Vec<SessionEvent>> {
        let path = self.current_session()?.clone();
        if !self.recent_cache.contains_key(&path) {
            let events = parser::parse_recent(&path, 6).unwrap_or_default();
            self.recent_cache.insert(path.clone(), events);
        }
        self.recent_cache.get(&path)
    }

    fn move_down(&mut self) {
        match self.focus {
            Pane::Projects => {
                move_state(&mut self.project_state, self.projects.len(), 1);
                self.refresh_sessions();
            }
            Pane::Sessions => move_state(&mut self.session_state, self.sessions.len(), 1),
            Pane::Preview => {}
        }
    }

    fn move_up(&mut self) {
        match self.focus {
            Pane::Projects => {
                move_state(&mut self.project_state, self.projects.len(), -1);
                self.refresh_sessions();
            }
            Pane::Sessions => move_state(&mut self.session_state, self.sessions.len(), -1),
            Pane::Preview => {}
        }
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Pane::Projects => Pane::Sessions,
            Pane::Sessions => Pane::Preview,
            Pane::Preview => Pane::Projects,
        };
    }

    fn request_resume(&mut self) {
        let stem = self
            .current_session()
            .and_then(|p| p.file_stem().and_then(|s| s.to_str()))
            .map(|s| s.to_string());
        if let Some(stem) = stem {
            self.resume_request = Some(stem);
        }
    }
}

fn move_state(state: &mut ListState, len: usize, delta: i32) {
    if len == 0 {
        state.select(None);
        return;
    }
    let cur = state.selected().unwrap_or(0) as i32;
    let new = (cur + delta).rem_euclid(len as i32);
    state.select(Some(new as usize));
}

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn main() -> Result<()> {
    let projects = discovery::list_projects().unwrap_or_default();
    let mut app = App::new(projects);
    app.refresh_sessions();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res?;

    if let Some(session_id) = app.resume_request {
        use std::os::unix::process::CommandExt;
        let err = Command::new("claude")
            .arg("--resume")
            .arg(&session_id)
            .exec();
        eprintln!("recall: failed to exec claude: {}", err);
        std::process::exit(1);
    }

    Ok(())
}

fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| draw(f, app))?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                    KeyCode::Tab => app.cycle_focus(),
                    KeyCode::Enter => match app.focus {
                        Pane::Projects => app.focus = Pane::Sessions,
                        Pane::Sessions => {
                            app.request_resume();
                            if app.resume_request.is_some() {
                                return Ok(());
                            }
                        }
                        Pane::Preview => {}
                    },
                    _ => {}
                }
            }
        }
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(35),
            Constraint::Percentage(40),
        ])
        .split(f.area());

    draw_projects(f, chunks[0], app);
    draw_sessions(f, chunks[1], app);
    draw_preview(f, chunks[2], app);
}

fn draw_projects(f: &mut Frame, area: Rect, app: &mut App) {
    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            ListItem::new(format!(
                "{} ({})",
                display_project(&p.encoded_cwd),
                p.session_count
            ))
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
            let short = id.split('-').next().unwrap_or(id);
            let when = mtime(p)
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| format_relative(d.as_secs() as i64))
                .unwrap_or_else(|| "?".to_string());
            ListItem::new(format!("{:<8}  {}", short, when))
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
            lines.push(Line::from(format!("cwd:      {}", cwd)));
        }
        lines.push(Line::from(format!("messages: {}", meta.message_count)));
        lines.push(Line::from(format!(
            "mtime:    {}",
            format_relative(meta.last_modified_unix)
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "── recent ──",
            Style::default().add_modifier(Modifier::DIM),
        )));
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
            Style::default().add_modifier(Modifier::DIM),
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
            ("[tool]   ", format!("{}({})", name, oneline(input)))
        }
        EventKind::System(t) => ("[system] ", oneline(t)),
    }
}

fn label_style_for(kind: &EventKind) -> Style {
    let base = Style::default().add_modifier(Modifier::BOLD);
    match kind {
        EventKind::UserText(_) | EventKind::UserToolResult { .. } => base,
        EventKind::AssistantText(_) => base,
        EventKind::AssistantToolUse { .. } => base.add_modifier(Modifier::DIM),
        EventKind::System(_) => base.add_modifier(Modifier::DIM),
    }
}

fn oneline(s: impl AsRef<str>) -> String {
    s.as_ref().replace('\n', " ").replace('\r', " ")
}

fn border(title: &str, focused: bool) -> Block<'_> {
    let style = if focused {
        Style::default().add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Block::default()
        .title(Span::styled(title, style))
        .borders(Borders::ALL)
        .border_style(style)
}

fn display_project(encoded: &str) -> String {
    encoded.strip_prefix('-').unwrap_or(encoded).to_string()
}

fn format_relative(unix_seconds: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let delta = now - unix_seconds;
    if delta < 60 {
        format!("{}s ago", delta)
    } else if delta < 3600 {
        format!("{}m ago", delta / 60)
    } else if delta < 86400 {
        format!("{}h ago", delta / 3600)
    } else {
        format!("{}d ago", delta / 86400)
    }
}
