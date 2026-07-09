//! Entry point: install color-eyre, run the async event loop, and always
//! restore the terminal on the way out.

use std::time::Duration;

use crossterm::event::EventStream;
use futures_util::StreamExt;
use tokio::sync::mpsc;

use tui_net_seting::app::App;
use tui_net_seting::effect;
use tui_net_seting::event::{Event, Msg};
use tui_net_seting::tui;
use tui_net_seting::ui;

/// Channel depth: worker results are drained one per loop turn, so a modest
/// buffer is plenty.
const CHANNEL_CAP: usize = 64;
const TICK: Duration = Duration::from_millis(250);

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let mut terminal = tui::init()?;
    let result = run(&mut terminal).await;
    // Restore the terminal on every path before propagating any error.
    tui::restore()?;
    result
}

async fn run(terminal: &mut tui::Tui) -> color_eyre::Result<()> {
    let mut app = App::new();
    let (tx, mut rx) = mpsc::channel::<Msg>(CHANNEL_CAP);

    // Kick off the initial loads and the change monitor.
    for eff in app.initial_effects() {
        effect::run(eff, tx.clone());
    }

    let mut input = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    terminal.draw(|f| ui::draw(f, &app))?;

    loop {
        let event = tokio::select! {
            maybe_input = input.next() => match maybe_input {
                Some(Ok(ev)) => Event::Input(ev),
                Some(Err(_)) => continue, // transient read error; keep going
                None => break,            // stdin closed
            },
            maybe_msg = rx.recv() => match maybe_msg {
                Some(msg) => Event::Msg(msg),
                None => break,            // all senders dropped (shouldn't happen)
            },
            _ = ticker.tick() => Event::Tick,
        };

        let effects = match event {
            Event::Input(ev) => match ui::keys::translate(&ev, &app) {
                Some(action) => app.apply(action),
                None => Vec::new(),
            },
            Event::Msg(msg) => app.on_msg(msg),
            Event::Tick => app.on_tick(),
        };

        for eff in effects {
            effect::run(eff, tx.clone());
        }

        if app.should_quit {
            break;
        }
        if std::mem::take(&mut app.dirty) {
            terminal.draw(|f| ui::draw(f, &app))?;
        }
    }

    Ok(())
}
