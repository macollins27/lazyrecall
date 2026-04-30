use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Terminal;
use recall_core::{discovery, Project};

fn main() -> Result<()> {
    let projects = discovery::list_projects().unwrap_or_default();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_loop(&mut terminal, &projects);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_loop<B: Backend>(terminal: &mut Terminal<B>, projects: &[Project]) -> Result<()> {
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(25),
                    Constraint::Percentage(35),
                    Constraint::Percentage(40),
                ])
                .split(f.area());

            let project_items: Vec<ListItem> = projects
                .iter()
                .map(|p| ListItem::new(format!("{} ({})", p.encoded_cwd, p.session_count)))
                .collect();
            let project_list = List::new(project_items)
                .block(
                    Block::default()
                        .title(" projects ")
                        .borders(Borders::ALL),
                )
                .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
            f.render_widget(project_list, chunks[0]);

            let sessions = Paragraph::new("(sessions — wire up next)")
                .block(Block::default().title(" sessions ").borders(Borders::ALL));
            f.render_widget(sessions, chunks[1]);

            let preview = Paragraph::new("(preview — wire up next)\n\nq to quit")
                .block(Block::default().title(" preview ").borders(Borders::ALL));
            f.render_widget(preview, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    return Ok(());
                }
            }
        }
    }
}
