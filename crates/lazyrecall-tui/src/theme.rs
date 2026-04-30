//! Color palette and reusable styles for the TUI.
//!
//! Mixes named ANSI colors (which adapt to the user's terminal theme) for
//! borders and accents with `Color::Indexed` from the xterm 256-color palette
//! for content. Inspired by lazygit's loud-on-borders, calmer-on-content
//! pattern.

use ratatui::style::{Color, Modifier, Style};

// Borders.
pub const BORDER: Color = Color::Indexed(243); // dim grey when unfocused
pub const BORDER_FOCUSED: Color = Color::Yellow; // lazygit-classic active border
pub const PANE_NUMBER: Color = Color::Indexed(214); // amber, [1] [2] [3]

// Role colors for the preview pane labels.
pub const USER: Color = Color::Indexed(51); // bright cyan
pub const ASSISTANT: Color = Color::Indexed(82); // lime green
pub const TOOL: Color = Color::Indexed(214); // amber
pub const RESULT: Color = Color::Indexed(245); // mid grey
pub const SYSTEM: Color = Color::Indexed(207); // pink-purple

// List item highlighting.
pub const SELECTED_BG: Color = Color::Indexed(238); // dim charcoal
pub const SELECTED_FG: Color = Color::Indexed(231); // near-white

// Status bar.
pub const STATUS_OK: Color = Color::Indexed(82); // lime
pub const STATUS_WARN: Color = Color::Indexed(214); // amber
pub const STATUS_WORK: Color = Color::Indexed(75); // light blue (in-flight)

// Help footer.
pub const HELP_KEY: Color = Color::Indexed(214); // amber, like lazygit's tips
pub const HELP_LABEL: Color = Color::Indexed(245); // mid grey

// Scrollbar.
pub const SCROLLBAR: Color = Color::Indexed(240); // grey track
pub const SCROLLBAR_THUMB: Color = Color::Yellow; // matches focused border

pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

pub fn selected() -> Style {
    Style::default()
        .bg(SELECTED_BG)
        .fg(SELECTED_FG)
        .add_modifier(Modifier::BOLD)
}
