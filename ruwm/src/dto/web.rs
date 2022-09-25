use core::fmt::Debug;

use serde::{Deserialize, Serialize};

use heapless::String;

use edge_frame::dto::Role;

use super::battery::BatteryState;
use super::valve::{ValveCommand, ValveState};
use super::water_meter::{WaterMeterCommand, WaterMeterState};

pub type RequestId = usize;

pub const USERNAME_MAX_LEN: usize = 32;
pub const PASSWORD_MAX_LEN: usize = 32;

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct WebRequest {
    id: RequestId,
    payload: WebRequestPayload,
}

impl WebRequest {
    pub fn new(id: RequestId, payload: WebRequestPayload) -> Self {
        Self { id, payload }
    }

    pub fn id(&self) -> RequestId {
        self.id
    }

    pub fn payload(&self) -> &WebRequestPayload {
        &self.payload
    }

    pub fn response(&self, role: Role) -> WebResponse {
        if role >= self.payload().role() {
            self.accept()
        } else {
            self.deny()
        }
    }

    pub fn accept(&self) -> WebResponse {
        WebResponse::Accepted(self.id)
    }

    pub fn deny(&self) -> WebResponse {
        WebResponse::Denied(self.id)
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum WebRequestPayload {
    Authenticate(String<USERNAME_MAX_LEN>, String<PASSWORD_MAX_LEN>),
    Logout,

    ValveCommand(ValveCommand),
    ValveStateRequest,

    WaterMeterCommand(WaterMeterCommand),
    WaterMeterStateRequest,

    BatteryStateRequest,

    WifiStatusRequest,
}

impl WebRequestPayload {
    pub fn role(&self) -> Role {
        match self {
            Self::Authenticate(_, _) => Role::None,
            Self::Logout => Role::None,
            Self::ValveStateRequest => Role::User,
            Self::WaterMeterStateRequest => Role::User,
            Self::BatteryStateRequest => Role::User,
            Self::WifiStatusRequest => Role::Admin,
            Self::ValveCommand(_) => Role::User,
            Self::WaterMeterCommand(_) => Role::User,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum WebResponse {
    Accepted(RequestId),
    Denied(RequestId),
}

impl WebResponse {
    pub fn id(&self) -> RequestId {
        match self {
            WebResponse::Accepted(id) => *id,
            WebResponse::Denied(id) => *id,
        }
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self, WebResponse::Accepted(_))
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum WebEvent {
    Response(WebResponse),

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
            Self::Response(_) => Role::None,
            Self::AuthenticationFailed => Role::None,
            Self::RoleState(_) => Role::None,
            Self::ValveState(_) => Role::User,
            Self::WaterMeterState(_) => Role::User,
            Self::BatteryState(_) => Role::User,
            //Self::WifiState(_) => Role::User,
        }
    }
}
