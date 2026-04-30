//! lazyrecall: lazygit-style TUI for Claude Code sessions.
//!
//! Three threads run concurrently, each with its own `Index` connection:
//! 1. Main thread — ratatui event loop (this file).
//! 2. Summarizer worker — polls `index.missing_summaries()`, calls Haiku in
//!    parallel (bounded), writes summaries back. Spawned below.
//! 3. FS watcher — debounced recursive watch on `~/.claude/projects/`,
//!    touches the index when new sessions appear. Spawned below.
//!
//! On Enter in the Sessions pane the run loop returns; we tear down the
//! terminal, then `exec` `claude --resume <id>` so the claude process
//! _replaces_ this one (Unix `exec`). The session's recorded cwd is captured
//! so we can chdir before exec — `claude --resume` scopes its session lookup
//! to the cwd it runs in.

mod app;
mod format;
mod input;
mod theme;
mod ui;

use std::io;
use std::process::Command;

use anyhow::Result;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use lazyrecall_core::{discovery, summarizer_worker, watcher, Index, Summarizer};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{load_api_key, seed_index, App};

fn main() -> Result<()> {
    let index_path = Index::default_path()?;
    let index = Index::open(&index_path)?;

    let projects = discovery::list_projects().unwrap_or_default();
    seed_index(&index, &projects);

    spawn_watcher(&index_path);
    let api_key = load_api_key();
    let api_key_set = api_key.is_some();
    if let Some(api_key) = api_key {
        spawn_summarizer(&index_path, api_key);
    }

    let mut app = App::new(projects, index, api_key_set);
    app.refresh_sessions();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = input::run_loop(&mut terminal, &mut app);

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
        eprintln!("lazyrecall: failed to exec claude: {err}");
        std::process::exit(1);
    }

    Ok(())
}

fn spawn_watcher(index_path: &std::path::Path) {
    let Ok(projects_root) = discovery::projects_root() else {
        return;
    };
    let watcher_index_path = index_path.to_path_buf();
    std::thread::spawn(move || {
        let watcher_index = match Index::open(&watcher_index_path) {
            Ok(idx) => idx,
            Err(e) => {
                lazyrecall_core::log::error("watcher", format!("could not open index: {e}"));
                return;
            }
        };
        if let Err(e) = watcher::run(&projects_root, watcher_index) {
            lazyrecall_core::log::error("watcher", format!("exited: {e}"));
        }
    });
}

fn spawn_summarizer(index_path: &std::path::Path, api_key: String) {
    let worker_index_path = index_path.to_path_buf();
    std::thread::spawn(move || {
        let worker_index = match Index::open(&worker_index_path) {
            Ok(idx) => idx,
            Err(e) => {
                lazyrecall_core::log::error("summarizer", format!("could not open index: {e}"));
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
                lazyrecall_core::log::error("summarizer", format!("tokio init failed: {e}"));
                return;
            }
        };
        let _ = rt.block_on(summarizer_worker::run(worker_index, summarizer));
    });
}
