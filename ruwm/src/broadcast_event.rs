use embedded_svc::mqtt::client::asyncs::MessageId;
use serde::{Deserialize, Serialize};

use crate::battery::BatteryState;
use crate::button::ButtonCommand;
use crate::mqtt::MqttClientNotification;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};
use crate::web::ConnectionId;
use crate::web_dto::WebEvent;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub struct BroadcastEvent {
    source: &'static str,
    payload: Payload,
}

impl BroadcastEvent {
    pub fn new(source: &'static str, payload: Payload) -> Self {
        Self { source, payload }
    }

    pub fn source(&self) -> &str {
        self.source
    }

    pub fn payload(&self) -> &Payload {
        &self.payload
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Serialize, Deserialize)]
pub enum Payload {
    ValveCommand(ValveCommand),
    ValveState(Option<ValveState>),

    WaterMeterCommand(WaterMeterCommand),
    WaterMeterState(WaterMeterState),

    BatteryState(BatteryState),

    ButtonCommand(ButtonCommand),

    WifiStatus,

    MqttPublishNotification(MessageId),
    MqttClientNotification(MqttClientNotification),

    WebResponse(ConnectionId, WebEvent),
}

impl From<BroadcastEvent> for Option<ValveCommand> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::ValveCommand(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<Option<ValveState>> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::ValveState(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<WaterMeterCommand> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::WaterMeterCommand(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<WaterMeterState> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::WaterMeterState(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<BatteryState> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::BatteryState(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<ButtonCommand> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::ButtonCommand(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<()> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::WifiStatus => Some(()),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<(ConnectionId, WebEvent)> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::WebResponse(connection_id, event) => Some((*connection_id, *event)),
            _ => None,
        }
    }
}
