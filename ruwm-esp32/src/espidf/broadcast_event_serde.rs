use esp_idf_svc::eventloop::{
    EspEventFetchData, EspEventPostData, EspTypedEventDeserializer, EspTypedEventSerializer,
    EspTypedEventSource,
};

use ruwm::broadcast_event::BroadcastEvent;

#[derive(Clone)]
pub(crate) struct Serde;

impl EspTypedEventSource for Serde {
    fn source() -> *const esp_idf_sys::c_types::c_char {
        b"WATER_METER\0".as_ptr() as *const _
    }
}

impl EspTypedEventSerializer<BroadcastEvent> for Serde {
    fn serialize<R>(
        event: &BroadcastEvent,
        f: impl for<'a> FnOnce(&'a EspEventPostData) -> R,
    ) -> R {
        f(&unsafe { EspEventPostData::new(Self::source(), Self::event_id(), event) })
    }
}

impl EspTypedEventDeserializer<BroadcastEvent> for Serde {
    fn deserialize<R>(
        data: &EspEventFetchData,
        f: &mut impl for<'a> FnMut(&'a BroadcastEvent) -> R,
    ) -> R {
        f(unsafe { data.as_payload() })
    }
}
