//! Reusable rendering helpers shared across screens.

pub mod confirm;
pub mod form;
pub mod password;
pub mod spinner;
pub mod toast;

use ratatui::layout::{Constraint, Flex, Layout, Rect};

/// Centre a fixed-size rectangle inside `area` for modal overlays.
pub fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let [row] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [cell] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(row);
    cell
}
