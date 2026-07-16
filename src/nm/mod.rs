//! Everything that talks to nmcli or runs read-only network probes. No
//! ratatui imports allowed in this tree.

pub mod command;
pub mod error;
pub mod monitor;
pub mod probe;

pub use command::run_nmcli;
pub use error::{NmExit, NmcliError};
