//! Zero-copy parsing of nmcli output. Pure functions `&str` → borrowed rows;
//! no I/O and no TUI types in this tree.

pub mod connection;
pub mod device;
pub mod error;
pub mod terse;
pub mod wifi;

pub use error::ParseError;
