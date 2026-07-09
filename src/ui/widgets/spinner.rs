//! A braille spinner frame driven by the app's tick counter.

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub fn frame(tick: u64) -> &'static str {
    FRAMES[(tick as usize) % FRAMES.len()]
}
