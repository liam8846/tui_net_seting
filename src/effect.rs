//! Side effects requested by the pure state machine, and the executor that
//! runs them. This is the ONLY module that calls `tokio::spawn` for nmcli
//! work: each effect spawns a worker that runs nmcli, parses (zero-copy),
//! converts to owned models at the channel boundary, and sends a [`Msg`] back.

use tokio::sync::mpsc::Sender;

use crate::event::{AppError, Msg, OpKind};
use crate::model::{Connection, ConnectionDraft, Device, RadioState, WifiAp};
use crate::nm::command as cmd;
use crate::nm::{monitor, run_nmcli};
use crate::parse::connection::{parse_connections, parse_profile};
use crate::parse::device::parse_devices;
use crate::parse::wifi::{parse_enabled, parse_wifi_list};

pub enum Effect {
    LoadDevices,
    LoadDeviceDetail(String),
    LoadConnections,
    LoadConnectionProfile(String),
    LoadWifi { rescan: bool },
    LoadRadio,
    /// A mutating operation; its `OpKind` comes back in `Msg::CommandDone`.
    Run { op: OpKind, args: Vec<String> },
    SpawnMonitor,
}

/// Spawn a worker for one effect. Never blocks the caller.
pub fn run(effect: Effect, tx: Sender<Msg>) {
    match effect {
        Effect::LoadDevices => spawn_load(
            tx,
            cmd::args_device_status(),
            |raw| Ok(parse_devices(raw)?.into_iter().map(Device::from).collect()),
            Msg::Devices,
        ),
        Effect::LoadConnections => spawn_load(
            tx,
            cmd::args_connections(),
            |raw| Ok(parse_connections(raw)?.into_iter().map(Connection::from).collect()),
            Msg::Connections,
        ),
        Effect::LoadWifi { rescan } => spawn_load(
            tx,
            cmd::args_wifi_list(rescan),
            |raw| Ok(parse_wifi_list(raw)?.into_iter().map(WifiAp::from).collect()),
            Msg::WifiAps,
        ),
        Effect::LoadDeviceDetail(device) => {
            tokio::spawn(async move {
                let result = run_nmcli(&cmd::args_device_show(&device))
                    .await
                    .map_err(AppError::from);
                let _ = tx.send(Msg::DeviceDetail { device, result }).await;
            });
        }
        Effect::LoadConnectionProfile(uuid) => {
            tokio::spawn(async move {
                let result = load_profile(&uuid).await;
                let _ = tx.send(Msg::ConnectionProfile { uuid, result }).await;
            });
        }
        Effect::LoadRadio => {
            tokio::spawn(async move {
                let _ = tx.send(Msg::Radio(load_radio().await)).await;
            });
        }
        Effect::Run { op, args } => {
            tokio::spawn(async move {
                let result = run_nmcli(&args).await.map(|_| ()).map_err(AppError::from);
                let _ = tx.send(Msg::CommandDone { op, result }).await;
            });
        }
        Effect::SpawnMonitor => monitor::spawn(tx),
    }
}

/// Shared shape for the list loaders: run nmcli, `parse` its stdout zero-copy
/// into owned models, then `wrap` the whole `Result` (nmcli error or parse
/// error alike, fully preserved) into that list's own `Msg` variant — so the
/// correct loading flag always clears and fatal errors stay classifiable.
fn spawn_load<T, P>(
    tx: Sender<Msg>,
    args: Vec<String>,
    parse: P,
    wrap: fn(Result<Vec<T>, AppError>) -> Msg,
) where
    T: Send + 'static,
    P: FnOnce(&str) -> Result<Vec<T>, AppError> + Send + 'static,
{
    tokio::spawn(async move {
        let result = match run_nmcli(&args).await {
            Ok(raw) => parse(&raw),
            Err(e) => Err(AppError::from(e)),
        };
        let _ = tx.send(wrap(result)).await;
    });
}

async fn load_profile(uuid: &str) -> Result<Box<ConnectionDraft>, AppError> {
    let raw = run_nmcli(&cmd::args_connection_profile(uuid)).await?;
    let profile = parse_profile(&raw)?;
    Ok(Box::new(ConnectionDraft::from_profile(uuid, profile)))
}

async fn load_radio() -> Result<RadioState, AppError> {
    let networking = run_nmcli(&cmd::args_networking_status()).await?;
    let wifi = run_nmcli(&cmd::args_radio_wifi_status()).await?;
    Ok(RadioState {
        networking: parse_enabled(&networking),
        wifi: parse_enabled(&wifi),
    })
}
