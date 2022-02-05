use esp_idf_svc::eventloop::{EspEventFetchData, EspEventPostData, EspEventSubscribeMetadata};

use ruwm::battery::BatteryState;
use ruwm::mqtt_recv::{MqttClientNotification, MqttCommand};
use ruwm::valve::{ValveCommand, ValveState};
use ruwm::water_meter::{WaterMeterCommand, WaterMeterState};

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Event {
    ValveCommand(ValveCommand),
    Valvestate(Option<ValveState>),

    WaterMeterCommand(WaterMeterCommand),
    WaterMeterState(WaterMeterState),

    BatteryState(BatteryState),

    MqttCommand(MqttCommand),
    MqttClientNotification(MqttClientNotification),
}

impl EspEventSubscribeMetadata for Event {
    fn source() -> *const esp_idf_sys::c_types::c_char {
        b"WATER_METER\0".as_ptr() as *const _
    }
}

impl From<EspEventFetchData> for Event {
    fn from(esp_event: EspEventFetchData) -> Self {
        unsafe { esp_event.as_payload() }
    }
}

impl<'a> From<&'a Event> for EspEventPostData<'a> {
    fn from(event: &Event) -> EspEventPostData<'_> {
        unsafe { EspEventPostData::new(Event::source(), Event::event_id(), event) }
    }
}
