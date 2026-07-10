//! Owned (`'static`) domain types held in App state, plus the
//! `From<XxxRow<'_>>` conversions that form the single borrowed→owned
//! boundary of the zero-copy parse layer.

pub mod connection;
pub mod device;
pub mod wifi;

pub use connection::{
    Connection, ConnectionDraft, ConnectionType, IpDraft, IpMethod, validate_address_field,
    validate_cidr, validate_dns_field, validate_gateway_field, validate_ip, validate_wifi_password,
};
pub use device::{Device, DeviceState, DeviceType};
pub use wifi::{RadioState, WifiAp};
