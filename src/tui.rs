//! Terminal lifecycle: raw mode + alternate screen enter/leave, and a panic
//! hook that restores the terminal *before* the (color-eyre) report prints, so
//! a panic never leaves the user's shell in raw mode.

use std::io::{self, Stdout};

use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Enter the alternate screen, enable raw mode, and install the panic hook.
pub fn init() -> color_eyre::Result<Tui> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    install_panic_hook();
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    Ok(terminal)
}

/// Leave the alternate screen and disable raw mode. Best-effort; safe to call
/// on any exit path, including after an error.
pub fn restore() -> color_eyre::Result<()> {
    execute!(io::stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

/// Chain a terminal-restoring step in front of the existing panic hook so the
/// report is printed on a sane terminal.
fn install_panic_hook() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort restore; ignore errors — we're already panicking.
        let _ = restore();
        hook(info);
    }));
}
