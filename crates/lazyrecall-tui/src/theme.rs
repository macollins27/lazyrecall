//! Color palette and reusable styles for the TUI.
//!
//! Colors are picked from the xterm 256-color palette via `Color::Indexed`
//! so they render predictably across terminal themes (light or dark) without
//! requiring TrueColor support. Modifiers (BOLD, DIM, REVERSED) are layered
//! on top to convey emphasis.

use ratatui::style::{Color, Modifier, Style};

// Role colors. Indexed values picked to look reasonable on both light and
// dark backgrounds.
pub const USER: Color = Color::Indexed(45); // soft cyan
pub const ASSISTANT: Color = Color::Indexed(114); // soft green
pub const TOOL: Color = Color::Indexed(179); // amber
pub const RESULT: Color = Color::Indexed(244); // mid grey
pub const SYSTEM: Color = Color::Indexed(141); // soft purple
pub const ACCENT: Color = Color::Indexed(75); // light blue (focused border)

// Status indicator colors.
pub const OK: Color = Color::Indexed(114); // green
pub const WARN: Color = Color::Indexed(179); // amber

pub fn dim() -> Style {
    Style::default().add_modifier(Modifier::DIM)
}

pub fn focused_border() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}
