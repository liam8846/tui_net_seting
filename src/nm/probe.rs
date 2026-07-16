//! Read-only network probe commands outside nmcli.

use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

pub const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

pub fn args_ping(target: &str, ipv6: bool) -> Vec<String> {
    let mut args = Vec::with_capacity(7);
    if ipv6 {
        args.push("-6".into());
    }
    args.extend(["-n", "-c", "2", "-W", "2", target].map(String::from));
    args
}

pub fn args_getent_hosts(host: &str) -> Vec<String> {
    vec!["hosts".into(), host.into()]
}

#[derive(Debug)]
pub struct ProbeOutput {
    pub success: bool,
    pub stdout: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("探測命令無法啟動")]
    Spawn(#[source] std::io::Error),
    #[error("逾時")]
    Timeout,
    #[error("探測命令輸出不是 UTF-8")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Run a read-only external network probe with a hard timeout.
pub async fn run(program: &str, args: &[String]) -> Result<ProbeOutput, ProbeError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let output = match tokio::time::timeout(PROBE_TIMEOUT, command.output()).await {
        Err(_) => return Err(ProbeError::Timeout),
        Ok(Err(error)) => return Err(ProbeError::Spawn(error)),
        Ok(Ok(output)) => output,
    };

    Ok(ProbeOutput {
        success: output.status.success(),
        stdout: String::from_utf8(output.stdout)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn ping_argv_supports_ipv4_and_ipv6() {
        assert_eq!(
            args_ping("192.0.2.1", false),
            ["-n", "-c", "2", "-W", "2", "192.0.2.1"]
        );
        assert_eq!(
            args_ping("2001:db8::1", true),
            ["-6", "-n", "-c", "2", "-W", "2", "2001:db8::1"]
        );
    }

    #[test]
    fn getent_hosts_argv() {
        assert_eq!(args_getent_hosts("example.com"), ["hosts", "example.com"]);
    }
}
