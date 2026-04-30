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
use recall_core::{
    discovery, parser, summarizer_worker, watcher, EventKind, Index, IndexStats, Project,
    Summarizer,
};
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
    summary_cache: HashMap<String, String>,
    focus: Pane,
    resume_request: Option<(String, Option<String>)>,
    last_loaded_project_idx: Option<usize>,
    index: Index,
    stats: IndexStats,
    api_key_set: bool,
}

impl App {
    fn new(projects: Vec<Project>, index: Index, api_key_set: bool) -> Self {
        let mut project_state = ListState::default();
        if !projects.is_empty() {
            project_state.select(Some(0));
        }
        let stats = index.stats().unwrap_or_default();
        Self {
            projects,
            project_state,
            sessions: Vec::new(),
            session_state: ListState::default(),
            metadata_cache: HashMap::new(),
            recent_cache: HashMap::new(),
            summary_cache: HashMap::new(),
            focus: Pane::Projects,
            resume_request: None,
            last_loaded_project_idx: None,
            index,
            stats,
            api_key_set,
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
        self.sessions.sort_by(|a, b| mtime(b).cmp(&mtime(a)));
        self.session_state = ListState::default();
        if !self.sessions.is_empty() {
            self.session_state.select(Some(0));
        }
        self.last_loaded_project_idx = Some(idx);
        self.refresh_index_state();
    }

    fn refresh_index_state(&mut self) {
        if let Ok(stats) = self.index.stats() {
            self.stats = stats;
        }
        let Some(idx) = self.last_loaded_project_idx else {
            return;
        };
        let Some(project) = self.projects.get(idx) else {
            return;
        };
        if let Ok(summaries) = self.index.project_summaries(&project.encoded_cwd) {
            self.summary_cache = summaries;
        }
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
                let project_key = self
                    .last_loaded_project_idx
                    .and_then(|i| self.projects.get(i))
                    .map(|p| p.encoded_cwd.clone())
                    .unwrap_or_default();
                let _ = self.index.upsert_session(&project_key, &path, &meta);
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

    fn cycle_focus_back(&mut self) {
        self.focus = match self.focus {
            Pane::Projects => Pane::Preview,
            Pane::Sessions => Pane::Projects,
            Pane::Preview => Pane::Sessions,
        };
    }

    fn request_resume(&mut self) {
        let stem = self
            .current_session()
            .and_then(|p| p.file_stem().and_then(|s| s.to_str()))
            .map(|s| s.to_string());
        let Some(stem) = stem else { return };
        // claude --resume scopes lookup to the cwd it runs in. Capture the
        // session's recorded cwd so main() can chdir before exec — otherwise
        // claude searches the wrong project dir and reports "transcript does
        // not exist".
        let metadata_cwd = self
            .current_session_metadata()
            .and_then(|m| m.cwd.clone());
        let cwd = metadata_cwd.or_else(|| {
            self.last_loaded_project_idx
                .and_then(|i| self.projects.get(i))
                .and_then(|p| p.real_cwd.clone())
        });
        self.resume_request = Some((stem, cwd));
    }
}

/// Load the Anthropic API key. Prefers ANTHROPIC_API_KEY in the environment;
/// falls back to a key stored at ~/.recall/api-key (one line, mode 0600 recommended).
/// The file-based path keeps the secret out of shell history and shell config.
fn load_api_key() -> Option<String> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        let trimmed = k.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home)
        .join(".recall")
        .join("api-key");
    let content = std::fs::read_to_string(&path).ok()?;
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn seed_index(index: &Index, projects: &[Project]) {
    for project in projects {
        let Ok(sessions) = discovery::list_sessions(project) else {
            continue;
        };
        for path in sessions {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let mtime_unix = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let _ = index.touch_session(&project.encoded_cwd, id, &path, mtime_unix);
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
    let index_path = Index::default_path()?;
    let index = Index::open(&index_path)?;

    let projects = discovery::list_projects().unwrap_or_default();
    seed_index(&index, &projects);

    if let Ok(projects_root) = discovery::projects_root() {
        let watcher_index_path = index_path.clone();
        std::thread::spawn(move || {
            let watcher_index = match Index::open(&watcher_index_path) {
                Ok(idx) => idx,
                Err(e) => {
                    eprintln!("recall: watcher could not open index: {}", e);
                    return;
                }
            };
            if let Err(e) = watcher::run(&projects_root, watcher_index) {
                eprintln!("recall: watcher exited: {}", e);
            }
        });
    }

    let api_key = load_api_key();
    let api_key_set = api_key.is_some();
    if let Some(api_key) = api_key {
        let worker_index_path = index_path.clone();
        std::thread::spawn(move || {
            let worker_index = match Index::open(&worker_index_path) {
                Ok(idx) => idx,
                Err(e) => {
                    eprintln!("recall: summarizer worker could not open index: {}", e);
                    return;
                }
            };
            let summarizer = Summarizer::new(api_key);
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("recall: summarizer worker tokio init failed: {}", e);
                    return;
                }
            };
            let _ = rt.block_on(summarizer_worker::run(worker_index, summarizer));
        });
    }

    let mut app = App::new(projects, index, api_key_set);
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

    if let Some((session_id, cwd)) = app.resume_request {
        use std::os::unix::process::CommandExt;
        let mut cmd = Command::new("claude");
        cmd.arg("--resume").arg(&session_id);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        let err = cmd.exec();
        eprintln!("recall: failed to exec claude: {}", err);
        std::process::exit(1);
    }

    Ok(())
}

fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut ticks_since_refresh: u32 = 0;
    const TICKS_PER_REFRESH: u32 = 25; // ~5s at 200ms poll
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
                    KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.cycle_focus(),
                    KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                        app.cycle_focus_back()
                    }
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

        ticks_since_refresh += 1;
        if ticks_since_refresh >= TICKS_PER_REFRESH {
            ticks_since_refresh = 0;
            app.refresh_index_state();
        }
    }
}

fn draw(f: &mut Frame, app: &mut App) {
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
    let summarizer_label = if !app.api_key_set {
        "no ANTHROPIC_API_KEY"
    } else if pending == 0 {
        "summarizer idle"
    } else {
        "summarizer working"
    };
    let body = format!(
        " {} sessions · {} summarized · {} pending · {}",
        app.stats.total, app.stats.summarized, pending, summarizer_label
    );
    let p = Paragraph::new(body).style(Style::default().add_modifier(Modifier::DIM));
    f.render_widget(p, area);
}

fn draw_projects(f: &mut Frame, area: Rect, app: &mut App) {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let items: Vec<ListItem> = app
        .projects
        .iter()
        .map(|p| {
            let name = display_project(p);
            let count = format!("({})", p.session_count);
            let when = p
                .latest_mtime_unix
                .map(format_relative)
                .unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::raw(name),
                Span::raw(" "),
                Span::styled(count, dim),
                Span::raw("  "),
                Span::styled(when, dim),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(border(" projects ", app.focus == Pane::Projects))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(list, area, &mut app.project_state);
}

fn draw_sessions(f: &mut Frame, area: Rect, app: &mut App) {
    let dim = Style::default().add_modifier(Modifier::DIM);
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
                    Span::styled(when, dim),
                ]),
                None => Line::from(Span::styled(when, dim)),
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

fn truncate_display(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(3)).collect();
        out.push_str("...");
        out
    }
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

fn display_project(project: &Project) -> String {
    if let Some(name) = &project.display_name {
        return name.clone();
    }
    project
        .encoded_cwd
        .strip_prefix('-')
        .unwrap_or(&project.encoded_cwd)
        .to_string()
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
