use core::fmt::Debug;

use embassy_time::Duration;

use embedded_graphics_core::pixelcolor::Rgb888;

use embedded_hal::digital::v2::{InputPin, OutputPin};

use channel_bridge::notification::Notification;

use static_cell::*;

use hal_sim::display::Display;
use hal_sim::gpio::{Input, Pin};

use ruwm::button::PressedLevel;
use ruwm::pulse_counter::PulseCounter;
use ruwm::pulse_counter::PulseWakeup;
use ruwm::screen::{buffer_size, Color, Flushable, OwnedDrawTargetExt};
use ruwm::valve::ValveState;
use ruwm::wm::WaterMeterState;
use ruwm::wm_stats::WaterMeterStatsState;

use crate::peripherals::ValvePeripherals;

pub static mut RTC_MEMORY: RtcMemory = RtcMemory::new();

#[derive(Default)]
pub struct RtcMemory {
    pub valve: Option<ValveState>,
    pub wm: WaterMeterState,
    pub wm_stats: WaterMeterStatsState,
}

impl RtcMemory {
    pub const fn new() -> Self {
        Self {
            valve: None,
            wm: WaterMeterState::new(),
            wm_stats: WaterMeterStatsState::new(),
        }
    }
}

pub fn valve_pins(
    peripherals: ValvePeripherals,
) -> (
    impl OutputPin<Error = impl Debug>,
    impl OutputPin<Error = impl Debug>,
    impl OutputPin<Error = impl Debug>,
) {
    let power = peripherals.power;
    let open = peripherals.open;
    let close = peripherals.close;

    (power, open, close)
}

#[cfg(feature = "nvs")]
pub fn storage(
    partition: EspDefaultNvsPartition,
) -> Result<
    &'static Mutex<
        impl embassy_sync::blocking_mutex::raw::RawMutex,
        RefCell<impl embedded_svc::storage::Storage>,
    >,
    InitError,
> {
    const POSTCARD_BUF_SIZE: usize = 500;

    struct PostcardSerDe;

    impl embedded_svc::storage::SerDe for PostcardSerDe {
        type Error = postcard::Error;

        fn serialize<'a, T>(&self, slice: &'a mut [u8], value: &T) -> Result<&'a [u8], Self::Error>
        where
            T: serde::Serialize,
        {
            postcard::to_slice(value, slice).map(|r| &*r)
        }

        fn deserialize<T>(&self, slice: &[u8]) -> Result<T, Self::Error>
        where
            T: serde::de::DeserializeOwned,
        {
            postcard::from_bytes(slice)
        }
    }

    static STORAGE: static_cell::StaticCell<
        Mutex<
            CriticalSectionRawMutex,
            RefCell<
                embedded_svc::storage::StorageImpl<
                    { POSTCARD_BUF_SIZE },
                    esp_idf_svc::nvs::EspDefaultNvs,
                    PostcardSerDe,
                >,
            >,
        >,
    > = static_cell::StaticCell::new();

    let storage = &*STORAGE.init(Mutex::new(RefCell::new(
        embedded_svc::storage::StorageImpl::new(
            esp_idf_svc::nvs::EspNvs::new(partition, "WM", true)?,
            PostcardSerDe,
        ),
    )));

    Ok(storage)
}

pub fn pulse(pulse: Pin<Input>) -> (impl PulseCounter, impl PulseWakeup) {
    static PULSE_SIGNAL: Notification = Notification::new();

    let pulse_counter = ruwm::pulse_counter::CpuPulseCounter::new(
        subscribe_pin(pulse, || PULSE_SIGNAL.notify()),
        PressedLevel::Low,
        &PULSE_SIGNAL,
        Some(Duration::from_millis(50)),
    );

    (pulse_counter, ())
}

pub fn button(
    pin: Pin<Input>,
    notification: &'static Notification,
) -> impl InputPin<Error = impl Debug> {
    subscribe_pin(pin, move || notification.notify())
}

pub fn display(
    display: Display<Rgb888>,
) -> impl Flushable<Color = Color, Error = impl Debug> + 'static {
    const DISPLAY_BUFFER_SIZE: usize = buffer_size::<Color>(crate::peripherals::DISPLAY_SIZE);

    static DISPLAY_BUFFER_1: StaticCell<[u8; DISPLAY_BUFFER_SIZE]> = StaticCell::new();
    static DISPLAY_BUFFER_2: StaticCell<[u8; DISPLAY_BUFFER_SIZE]> = StaticCell::new();

    display.owned_color_converted().owned_buffered(
        DISPLAY_BUFFER_1.init_with(|| [0_u8; DISPLAY_BUFFER_SIZE]),
        DISPLAY_BUFFER_2.init_with(|| [0_u8; DISPLAY_BUFFER_SIZE]),
    )
}

// pub fn mqtt() -> Result<
//     (
//         &'static str,
//         impl Client + Publish,
//         impl Connection<Message = Option<MqttCommand>>,
//     ),
//     InitError,
// > {
//     let client_id = "water-meter-demo";
//     let mut mqtt_parser = MessageParser::new();

//     let (mqtt_client, mqtt_conn) = EspMqttClient::new_with_converting_async_conn(
//         "mqtt://broker.emqx.io:1883",
//         &MqttClientConfiguration {
//             client_id: Some(client_id),
//             ..Default::default()
//         },
//         move |event| mqtt_parser.convert(event),
//     )?;

//     let mqtt_client = mqtt_client.into_async();

//     Ok((client_id, mqtt_client, mqtt_conn))
// }

fn subscribe_pin(
    mut pin: Pin<Input>,
    notify: impl Fn() + Send + 'static,
) -> impl InputPin<Error = impl Debug> {
    pin.subscribe(notify);

    pin
}
