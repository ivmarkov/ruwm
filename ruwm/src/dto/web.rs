use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use heapless::String;

use edge_frame::dto::Role;

use super::battery::BatteryState;
use super::valve::{ValveCommand, ValveState};
use super::water_meter::{WaterMeterCommand, WaterMeterState};

pub const USERNAME_MAX_LEN: usize = 32;
pub const PASSWORD_MAX_LEN: usize = 32;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum WebRequest {
    Authenticate(String<USERNAME_MAX_LEN>, String<PASSWORD_MAX_LEN>),
    Logout,

    ValveCommand(ValveCommand),
    WaterMeterCommand(WaterMeterCommand),
    // TODO
    //WifiSettingsUpdate(...),
}

impl WebRequest {
    pub fn role(&self) -> Role {
        match self {
            Self::Authenticate(_, _) => Role::None,
            Self::Logout => Role::None,
            Self::ValveCommand(_) => Role::User,
            Self::WaterMeterCommand(_) => Role::User,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum WebEvent {
    NoPermissions,

    AuthenticationFailed,

    RoleState(Role),
    ValveState(Option<ValveState>),
    WaterMeterState(WaterMeterState),
    BatteryState(BatteryState),
    //WifiState(Status),

    // MqttPublishNotification(MessageId),
    // MqttClientNotification(MqttClientNotification),
}

impl WebEvent {
    pub fn role(&self) -> Role {
        match self {
            Self::NoPermissions => Role::None,
            Self::AuthenticationFailed => Role::None,
            Self::RoleState(_) => Role::None,
            Self::ValveState(_) => Role::User,
            Self::WaterMeterState(_) => Role::User,
            Self::BatteryState(_) => Role::User,
            //Self::WifiState(_) => Role::User,
        }
    }
}
