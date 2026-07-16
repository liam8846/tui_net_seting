//! Side effects requested by the pure state machine, and the executor that
//! runs them. This is the ONLY module that calls `tokio::spawn` for nmcli and
//! probe work: each effect spawns a worker, parses nmcli output (zero-copy),
//! converts to owned models at the channel boundary, and sends a [`Msg`] back.

use tokio::sync::mpsc::Sender;

use crate::event::{AppError, ConnTestStage, Msg, OpKind, StageResult};
use crate::model::{Connection, ConnectionDraft, Device, RadioState, WifiAp};
use crate::nm::command as cmd;
use crate::nm::probe::{self, ProbeError};
use crate::nm::{NmcliError, monitor, run_nmcli};
use crate::parse::connection::{parse_connections, parse_profile};
use crate::parse::device::parse_devices;
use crate::parse::gateway::parse_gateway;
use crate::parse::wifi::{parse_enabled, parse_wifi_list};

pub enum Effect {
    LoadDevices,
    LoadDeviceDetail(String),
    LoadConnections,
    LoadConnectionProfile(String),
    LoadWifi {
        rescan: bool,
    },
    LoadRadio,
    /// A mutating operation; its `OpKind` comes back in `Msg::CommandDone`.
    Run {
        op: OpKind,
        args: Vec<String>,
    },
    RunConnTest {
        run_id: u64,
        device: Option<String>,
        check_wifi: bool,
    },
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
            |raw| {
                Ok(parse_connections(raw)?
                    .into_iter()
                    .map(Connection::from)
                    .collect())
            },
            Msg::Connections,
        ),
        Effect::LoadWifi { rescan } => spawn_load(
            tx,
            cmd::args_wifi_list(rescan),
            |raw| {
                Ok(parse_wifi_list(raw)?
                    .into_iter()
                    .map(WifiAp::from)
                    .collect())
            },
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
        Effect::RunConnTest {
            run_id,
            device,
            check_wifi,
        } => {
            tokio::spawn(run_conn_test(tx, run_id, device, check_wifi));
        }
        Effect::SpawnMonitor => monitor::spawn(tx),
    }
}

async fn run_conn_test(tx: Sender<Msg>, run_id: u64, device: Option<String>, check_wifi: bool) {
    if let Some(device) = device
        && !send_conn_test_stage(
            &tx,
            run_id,
            ConnTestStage::Gateway,
            test_gateway(&device).await,
        )
        .await
    {
        return;
    }

    if !send_conn_test_stage(&tx, run_id, ConnTestStage::Internet, test_internet().await).await {
        return;
    }
    if !send_conn_test_stage(&tx, run_id, ConnTestStage::Dns, test_dns().await).await {
        return;
    }
    if check_wifi {
        let _ = send_conn_test_stage(&tx, run_id, ConnTestStage::Wifi, test_wifi().await).await;
    }
}

async fn send_conn_test_stage(
    tx: &Sender<Msg>,
    run_id: u64,
    stage: ConnTestStage,
    result: StageResult,
) -> bool {
    tx.send(Msg::ConnTestStage {
        run_id,
        stage,
        result,
    })
    .await
    .is_ok()
}

async fn test_gateway(device: &str) -> StageResult {
    let raw = match run_nmcli(&cmd::args_gateway(device)).await {
        Ok(raw) => raw,
        Err(error) => return nmcli_failure(error, "無法查詢預設閘道"),
    };
    let gateways = parse_gateway(&raw);
    let (gateway, ipv6) = match (gateways.ipv4, gateways.ipv6) {
        (Some(gateway), _) => (gateway, false),
        (None, Some(gateway)) => (gateway, true),
        (None, None) => {
            return StageResult::Failed(format!("裝置 {device} 沒有預設閘道"));
        }
    };

    match probe::run("ping", &probe::args_ping(gateway, ipv6)).await {
        Ok(output) if output.success => StageResult::Passed(format!("閘道 {gateway} 回應正常")),
        Err(ProbeError::Timeout) => StageResult::Failed("逾時".into()),
        Ok(_) | Err(_) => StageResult::Failed(format!("閘道 {gateway} 無回應")),
    }
}

async fn test_internet() -> StageResult {
    match run_nmcli(&cmd::args_connectivity_check()).await {
        Ok(raw) => match raw.trim() {
            "full" => StageResult::Passed("可連上網際網路".into()),
            "portal" => StageResult::Failed("被入口網站攔截，需先登入 (portal)".into()),
            "limited" => StageResult::Failed("僅有受限連線".into()),
            "none" => StageResult::Failed("無法連上網際網路".into()),
            _ => test_internet_fallback().await,
        },
        Err(error) => nmcli_failure(error, "無法檢查網際網路連線"),
    }
}

async fn test_internet_fallback() -> StageResult {
    match probe::run("ping", &probe::args_ping("1.1.1.1", false)).await {
        Ok(output) if output.success => {
            StageResult::Passed("可連上 1.1.1.1（NM 連通性檢查未啟用）".into())
        }
        Err(ProbeError::Timeout) => StageResult::Failed("逾時".into()),
        Ok(_) | Err(_) => StageResult::Failed("無法連上 1.1.1.1".into()),
    }
}

async fn test_dns() -> StageResult {
    match probe::run("getent", &probe::args_getent_hosts("example.com")).await {
        Ok(output) if output.success && !output.stdout.trim().is_empty() => {
            StageResult::Passed("example.com 解析正常".into())
        }
        Err(ProbeError::Timeout) => StageResult::Failed("逾時".into()),
        Ok(_) | Err(_) => StageResult::Failed("無法解析 example.com（DNS 異常）".into()),
    }
}

async fn test_wifi() -> StageResult {
    let radio = match run_nmcli(&cmd::args_radio_wifi_status()).await {
        Ok(raw) => raw,
        Err(error) => return nmcli_failure(error, "無法讀取 Wi-Fi 無線電狀態"),
    };
    if !parse_enabled(&radio) {
        return StageResult::Failed("Wi-Fi 無線電已關閉".into());
    }

    let raw = match run_nmcli(&cmd::args_wifi_list(false)).await {
        Ok(raw) => raw,
        Err(error) => return nmcli_failure(error, "無法讀取 Wi-Fi 連線狀態"),
    };
    match parse_wifi_list(&raw) {
        Ok(rows) => match rows.into_iter().find(|row| row.in_use) {
            Some(row) => {
                StageResult::Passed(format!("已連線「{}」（訊號 {}%）", row.ssid, row.signal))
            }
            None => StageResult::Skipped("無線電開啟，未連接任何 Wi-Fi".into()),
        },
        Err(_) => StageResult::Failed("無法讀取 Wi-Fi 連線狀態".into()),
    }
}

fn nmcli_failure(error: NmcliError, message: &str) -> StageResult {
    if matches!(error, NmcliError::ClientTimeout(_)) {
        StageResult::Failed("逾時".into())
    } else {
        StageResult::Failed(message.into())
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
