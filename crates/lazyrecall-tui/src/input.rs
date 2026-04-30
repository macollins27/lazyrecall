//! Event loop and key handling.

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::backend::Backend;
use ratatui::Terminal;

use crate::app::{App, Pane};
use crate::ui::draw;

const POLL_INTERVAL: Duration = Duration::from_millis(200);
const TICKS_PER_REFRESH: u32 = 25; // ~5s at 200ms poll

pub fn run_loop<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    let mut ticks_since_refresh: u32 = 0;
    loop {
        terminal.draw(|f| draw(f, app))?;

        if event::poll(POLL_INTERVAL)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('j') | KeyCode::Down => app.move_down(),
                    KeyCode::Char('k') | KeyCode::Up => app.move_up(),
                    KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => app.cycle_focus(),
                    KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => app.cycle_focus_back(),
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
