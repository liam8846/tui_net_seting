//! Everything that talks to nmcli. No ratatui imports allowed in this tree.

pub mod command;
pub mod error;
pub mod monitor;

pub use command::run_nmcli;
pub use error::{NmExit, NmcliError};
