//! Long-lived `nmcli monitor` child used as a change signal.
//!
//! The monitor's output is human-oriented text, so lines are never parsed —
//! any output means "something changed". Bursts are coalesced (trailing-edge
//! debounce) into a single [`Msg::NetworkChanged`]. If the child dies the
//! task reports [`Msg::MonitorDied`] and restarts it with exponential backoff;
//! the app falls back to timer polling in the meantime.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc::Sender;

use crate::event::Msg;
use crate::nm::command::args_monitor;

const DEBOUNCE: Duration = Duration::from_millis(500);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

pub fn spawn(tx: Sender<Msg>) {
    tokio::spawn(async move {
        let mut backoff = INITIAL_BACKOFF;
        loop {
            let healthy = run_monitor(&tx).await.is_ok();
            if healthy {
                backoff = INITIAL_BACKOFF;
            }
            if tx.send(Msg::MonitorDied).await.is_err() {
                return; // app is shutting down
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    });
}

/// Runs one monitor child to completion. Ok(()) = clean EOF, Err = I/O trouble.
async fn run_monitor(tx: &Sender<Msg>) -> std::io::Result<()> {
    let mut child = Command::new("nmcli")
        .args(args_monitor())
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;

    let stdout = child.stdout.take().ok_or_else(|| {
        std::io::Error::other("nmcli monitor stdout not captured")
    })?;
    let _ = tx.send(Msg::MonitorUp).await;

    let mut lines = BufReader::new(stdout).lines();
    'outer: while (lines.next_line().await?).is_some() {
        // Coalesce the burst: keep draining until the child is quiet.
        loop {
            match tokio::time::timeout(DEBOUNCE, lines.next_line()).await {
                Ok(Ok(Some(_))) => continue,
                Ok(Ok(None)) | Ok(Err(_)) => {
                    let _ = tx.send(Msg::NetworkChanged).await;
                    break 'outer;
                }
                Err(_quiet) => break,
            }
        }
        if tx.send(Msg::NetworkChanged).await.is_err() {
            break;
        }
    }

    let _ = child.wait().await;
    Ok(())
}
