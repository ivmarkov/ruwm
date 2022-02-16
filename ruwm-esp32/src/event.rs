use embedded_svc::mqtt::client::MessageId;

use esp_idf_svc::eventloop::{
    EspEventFetchData, EspEventPostData, EspTypedEventDeserializer, EspTypedEventSerializer,
    EspTypedEventSource,
};

use ruwm::battery::BatteryState;
use ruwm::mqtt_recv::MqttClientNotification;
use ruwm::valve::{ValveCommand, ValveState};
use ruwm::water_meter::{WaterMeterCommand, WaterMeterState};

pub type ValveCommandEvent = SpecificEvent<ValveCommand, 1>;
pub type ValveStateEvent = SpecificEvent<Option<ValveState>, 2>;
pub type WaterMeterCommandEvent = SpecificEvent<WaterMeterCommand, 3>;
pub type WaterMeterStateEvent = SpecificEvent<WaterMeterState, 4>;
pub type BatteryStateEvent = SpecificEvent<BatteryState, 5>;
pub type MqttClientNotificationEvent = SpecificEvent<MqttClientNotification, 6>;
pub type MqttPublishNotificationEvent = SpecificEvent<MessageId, 7>;
pub type ValveSpinCommandEvent = SpecificEvent<ValveCommand, 8>;
pub type ValveSpinNotifEvent = SpecificEvent<(), 9>;
pub type WifiStatusNotifEvent = SpecificEvent<(), 10>;

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Event {
    ValveCommand(ValveCommand),
    ValveState(Option<ValveState>),

    WaterMeterCommand(WaterMeterCommand),
    WaterMeterState(WaterMeterState),

    BatteryState(BatteryState),

    MqttClientNotification(MqttClientNotification),
    MqttPublishNotification(MessageId),

    ValveSpinCommandEvent(ValveCommand),
    ValveSpinNotifEvent,

    WifiStatus,
}

impl EspTypedEventSource for Event {
    fn source() -> *const esp_idf_sys::c_types::c_char {
        b"WATER_METER\0".as_ptr() as *const _
    }
}

impl EspTypedEventSerializer<Event> for Event {
    fn serialize<R>(event: &Event, f: impl for<'a> FnOnce(&'a EspEventPostData) -> R) -> R {
        match event {
            Self::ValveCommand(payload) => ValveCommandEvent::serialize(payload, f),
            Self::ValveState(payload) => ValveStateEvent::serialize(payload, f),
            Self::WaterMeterCommand(payload) => WaterMeterCommandEvent::serialize(payload, f),
            Self::WaterMeterState(payload) => WaterMeterStateEvent::serialize(payload, f),
            Self::BatteryState(payload) => BatteryStateEvent::serialize(payload, f),
            Self::MqttClientNotification(payload) => {
                MqttClientNotificationEvent::serialize(payload, f)
            }
            Self::MqttPublishNotification(payload) => {
                MqttPublishNotificationEvent::serialize(payload, f)
            }
            Self::ValveSpinCommandEvent(payload) => ValveSpinCommandEvent::serialize(payload, f),
            Self::ValveSpinNotifEvent => ValveSpinNotifEvent::serialize(&(), f),
            Self::WifiStatus => WifiStatusNotifEvent::serialize(&(), f),
        }
    }
}

impl EspTypedEventDeserializer<Event> for Event {
    fn deserialize<R>(data: &EspEventFetchData, f: &mut impl for<'a> FnMut(&'a Event) -> R) -> R {
        let id = Some(data.event_id);

        let event = unsafe {
            if id == ValveCommandEvent::event_id() {
                Self::ValveCommand(*data.as_payload())
            } else if id == ValveStateEvent::event_id() {
                Self::ValveState(*data.as_payload())
            } else if id == WaterMeterCommandEvent::event_id() {
                Self::WaterMeterCommand(*data.as_payload())
            } else if id == WaterMeterStateEvent::event_id() {
                Self::WaterMeterState(*data.as_payload())
            } else if id == BatteryStateEvent::event_id() {
                Self::BatteryState(*data.as_payload())
            } else if id == MqttClientNotificationEvent::event_id() {
                Self::MqttClientNotification(*data.as_payload())
            } else if id == MqttPublishNotificationEvent::event_id() {
                Self::MqttPublishNotification(*data.as_payload())
            } else if id == ValveSpinCommandEvent::event_id() {
                Self::ValveSpinCommandEvent(*data.as_payload())
            } else if id == ValveSpinNotifEvent::event_id() {
                Self::ValveSpinNotifEvent
            } else if id == WifiStatusNotifEvent::event_id() {
                Self::WifiStatus
            } else {
                panic!("Unknown event ID: {:?}", id);
            }
        };

        f(&event)
    }
}

pub struct SpecificEvent<P, const N: i32>(P);

impl<P, const N: i32> EspTypedEventSource for SpecificEvent<P, N> {
    fn source() -> *const esp_idf_sys::c_types::c_char {
        Event::source()
    }

    fn event_id() -> Option<i32> {
        Some(N)
    }
}

impl<P, const N: i32> EspTypedEventSerializer<P> for SpecificEvent<P, N>
where
    P: Copy,
{
    fn serialize<R>(event: &P, f: impl for<'a> FnOnce(&'a EspEventPostData) -> R) -> R {
        f(&unsafe { EspEventPostData::new(Self::source(), Self::event_id(), event) })
    }
}

impl<P, const N: i32> EspTypedEventDeserializer<P> for SpecificEvent<P, N>
where
    P: Copy,
{
    fn deserialize<R>(data: &EspEventFetchData, f: &mut impl for<'a> FnMut(&'a P) -> R) -> R {
        f(unsafe { data.as_payload() })
    }
}
