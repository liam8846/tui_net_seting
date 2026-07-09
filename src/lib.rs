//! An async ratatui front-end for nmcli (NetworkManager CLI).
//!
//! Module layout mirrors responsibility, not screens:
//! * [`nm`]     — everything that talks to nmcli (no TUI types).
//! * [`parse`]  — zero-copy parsing of nmcli terse output (no I/O).
//! * [`model`]  — owned domain types + the borrowed→owned boundary.
//! * [`event`]  — the Event / Action / Msg enums.
//! * [`app`]    — pure state machine over that state.
//! * [`effect`] — the only place nmcli work is spawned.
//! * [`ui`]     — rendering (never mutates state, never does I/O).
//! * [`tui`]    — terminal lifecycle + panic safety.

#![deny(clippy::unwrap_used, clippy::expect_used)]
// Tests may unwrap/expect freely; the deny above is for production paths only.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod app;
pub mod effect;
pub mod event;
pub mod model;
pub mod nm;
pub mod parse;
pub mod tui;
pub mod ui;
