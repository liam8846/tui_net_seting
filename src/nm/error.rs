//! Typed errors for nmcli invocation, including the documented exit codes
//! from nmcli(1) "EXIT STATUS".

use std::time::Duration;

/// Exit codes documented in nmcli(1). Unknown codes map to `None` so newer
/// nmcli versions degrade to a generic message instead of a wrong label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NmExit {
    Unknown = 1,
    InvalidInput = 2,
    Timeout = 3,
    ActivationFailed = 4,
    DeactivationFailed = 5,
    DisconnectFailed = 6,
    DeletionFailed = 7,
    NmNotRunning = 8,
    NotFound = 10,
}

impl NmExit {
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            1 => Some(Self::Unknown),
            2 => Some(Self::InvalidInput),
            3 => Some(Self::Timeout),
            4 => Some(Self::ActivationFailed),
            5 => Some(Self::DeactivationFailed),
            6 => Some(Self::DisconnectFailed),
            7 => Some(Self::DeletionFailed),
            8 => Some(Self::NmNotRunning),
            10 => Some(Self::NotFound),
            _ => None,
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Unknown => "nmcli failed",
            Self::InvalidInput => "invalid input",
            Self::Timeout => "nmcli timed out waiting for the operation",
            Self::ActivationFailed => "connection activation failed",
            Self::DeactivationFailed => "connection deactivation failed",
            Self::DisconnectFailed => "disconnecting the device failed",
            Self::DeletionFailed => "connection deletion failed",
            Self::NmNotRunning => {
                "NetworkManager is not running (try `systemctl start NetworkManager`)"
            }
            Self::NotFound => "connection, device or access point not found",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NmcliError {
    #[error("failed to launch nmcli — is NetworkManager installed? ({0})")]
    Spawn(#[source] std::io::Error),

    #[error("{}", failed_display(exit, *code, stderr))]
    Failed {
        exit: Option<NmExit>,
        code: i32,
        stderr: String,
    },

    #[error("nmcli did not respond within {0:?}")]
    ClientTimeout(Duration),

    #[error("nmcli produced non-UTF-8 output")]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error("nmcli was terminated by a signal")]
    Killed,
}

impl NmcliError {
    /// True when nothing will work until NetworkManager itself is back.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::Spawn(_)
                | Self::Failed {
                    exit: Some(NmExit::NmNotRunning),
                    ..
                }
        )
    }

    /// True when a Wi-Fi activation failed because secrets (password) were
    /// missing or wrong — the caller should re-prompt.
    pub fn is_bad_secrets(&self) -> bool {
        matches!(
            self,
            Self::Failed { exit: Some(NmExit::ActivationFailed), stderr, .. }
                if stderr.to_ascii_lowercase().contains("secrets")
        )
    }
}

fn failed_display(exit: &Option<NmExit>, code: i32, stderr: &str) -> String {
    let base = match exit {
        Some(e) => e.describe().to_string(),
        None => format!("nmcli failed (exit code {code})"),
    };
    match stderr.lines().next().map(str::trim).filter(|l| !l.is_empty()) {
        Some(line) => format!("{base}: {line}"),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_code_maps_documented_codes() {
        assert_eq!(NmExit::from_code(3), Some(NmExit::Timeout));
        assert_eq!(NmExit::from_code(8), Some(NmExit::NmNotRunning));
        assert_eq!(NmExit::from_code(10), Some(NmExit::NotFound));
        assert_eq!(NmExit::from_code(99), None);
        assert_eq!(NmExit::from_code(0), None);
    }

    #[test]
    fn failed_message_includes_stderr_first_line() {
        let err = NmcliError::Failed {
            exit: Some(NmExit::ActivationFailed),
            code: 4,
            stderr: "Error: Secrets were required, but not provided.\nmore\n".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("activation failed"));
        assert!(msg.contains("Secrets were required"));
        assert!(!msg.contains("more"));
    }

    #[test]
    fn unknown_code_falls_back_to_generic() {
        let err = NmcliError::Failed { exit: None, code: 99, stderr: String::new() };
        assert_eq!(err.to_string(), "nmcli failed (exit code 99)");
    }

    #[test]
    fn bad_secrets_detection() {
        let err = NmcliError::Failed {
            exit: Some(NmExit::ActivationFailed),
            code: 4,
            stderr: "Error: Connection activation failed: Secrets were required".into(),
        };
        assert!(err.is_bad_secrets());
        let other = NmcliError::Failed {
            exit: Some(NmExit::NotFound),
            code: 10,
            stderr: "no network with SSID".into(),
        };
        assert!(!other.is_bad_secrets());
    }
}
