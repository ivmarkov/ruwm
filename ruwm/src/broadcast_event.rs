use embedded_svc::mqtt::client::asyncs::MessageId;

use crate::battery::BatteryState;
use crate::button::ButtonCommand;
use crate::mqtt::MqttClientNotification;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};
use crate::web::ConnectionId;
use crate::web_dto::WebEvent;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct WifiStatus;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Quit;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
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

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Payload {
    ValveCommand(ValveCommand),
    ValveState(Option<ValveState>),

    WaterMeterCommand(WaterMeterCommand),
    WaterMeterState(WaterMeterState),

    BatteryState(BatteryState),

    ButtonCommand(ButtonCommand),

    WifiStatus(WifiStatus),

    MqttPublishNotification(MessageId),
    MqttClientNotification(MqttClientNotification),

    WebResponse(ConnectionId, WebEvent),

    Quit(Quit),
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

impl From<BroadcastEvent> for Option<WifiStatus> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::WifiStatus(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<Quit> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::Quit(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<MqttClientNotification> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::MqttClientNotification(value) => Some(*value),
            _ => None,
        }
    }
}

impl From<BroadcastEvent> for Option<MessageId> {
    fn from(event: BroadcastEvent) -> Self {
        match event.payload() {
            Payload::MqttPublishNotification(value) => Some(*value),
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
