extern crate alloc;
use alloc::sync::Arc;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::timer::nonblocking::TimerService;
use embedded_svc::utils::nonblocking::Asyncify;
use embedded_svc::wifi::{ClientConfiguration, Configuration, Wifi};
use esp_idf_svc::netif::EspNetifStack;
use esp_idf_svc::nvs::EspDefaultNvs;
use esp_idf_svc::sysloop::EspSysLoopStack;
use esp_idf_svc::timer::EspTimerService;
use esp_idf_svc::wifi::EspWifi;
use esp_idf_sys::EspError;
use event::{ValveSpinCommandEvent, ValveSpinNotifEvent, WifiStatusNotifEvent};
use futures::future::join;

use embedded_svc::event_bus::nonblocking::{EventBus as _, PostboxProvider};

use esp_idf_hal::adc;
use esp_idf_hal::mutex::Mutex;
use esp_idf_hal::prelude::Peripherals;

use esp_idf_svc::eventloop::{
    EspBackgroundEventLoop, EspEventLoop, EspEventLoopType, EspTypedEventDeserializer,
    EspTypedEventSerializer,
};
use esp_idf_svc::mqtt::client::{EspMqttClient, MqttClientConfiguration};

use pulse_counter::PulseCounter;

use ruwm::battery::{self, BatteryState};
use ruwm::mqtt_recv;
use ruwm::mqtt_send;
use ruwm::pulse_counter::PulseCounter as _;
use ruwm::state_snapshot::StateSnapshot;
use ruwm::valve::{self, ValveState};
use ruwm::water_meter::{self, WaterMeterState};
use ruwm::{emergency, pipe};

use crate::event::{
    BatteryStateEvent, MqttClientNotificationEvent, MqttCommandEvent, MqttPublishNotificationEvent,
    ValveCommandEvent, ValveStateEvent, WaterMeterCommandEvent, WaterMeterStateEvent,
};

mod event;
mod event_logger;

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

const SSID: &str = env!("RUWM_WIFI_SSID");
const PASS: &str = env!("RUWM_WIFI_PASS");

fn state<S>() -> StateSnapshot<Mutex<S>>
where
    S: Send + Sync + Default,
{
    StateSnapshot::<Mutex<S>>::new()
}

fn receiver<D, P, T>(event_loop: &mut EspEventLoop<T>) -> Result<impl Receiver<Data = P>, EspError>
where
    T: EspEventLoopType + Send + 'static,
    D: EspTypedEventDeserializer<P>,
    P: Clone + Send + Sync + 'static,
{
    event_loop.as_typed::<D, _>().as_async().subscribe()
}

fn sender<D, P, T>(event_loop: &mut EspEventLoop<T>) -> Result<impl Sender<Data = P>, EspError>
where
    T: EspEventLoopType + Send + 'static,
    D: EspTypedEventSerializer<P>,
    P: Clone + Send + Sync + 'static,
{
    event_loop.as_typed::<D, _>().as_async().postbox()
}

fn main() -> anyhow::Result<()> {
    esp_idf_sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take().unwrap();

    let mut event_loop = EspBackgroundEventLoop::new(&Default::default())?;

    let mut timer_service = EspTimerService::new()?.into_async();

    let event_logger = event_logger::run(receiver::<event::Event, _, _>(&mut event_loop)?);

    let netif_stack = Arc::new(EspNetifStack::new()?);
    let sysloop_stack = Arc::new(EspSysLoopStack::new()?);
    let nvs_stack = Arc::new(EspDefaultNvs::new()?);

    let mut wifi = EspWifi::new(netif_stack, sysloop_stack, nvs_stack)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: SSID.into(),
        password: PASS.into(),
        ..Default::default()
    }))?;

    let wifi_notif = pipe::run(
        wifi.as_async().subscribe()?,
        sender::<WifiStatusNotifEvent, _, _>(&mut event_loop)?,
    );

    let valve_state = state::<Option<ValveState>>();
    let battery_state = state::<BatteryState>();
    let water_meter_state = state::<WaterMeterState>();

    let valve_power_pin = peripherals.pins.gpio10.into_output()?;
    let valve_open_pin = peripherals.pins.gpio12.into_output()?;
    let valve_close_pin = peripherals.pins.gpio13.into_output()?;

    let valve = valve::run(
        valve_state.clone(),
        receiver::<ValveCommandEvent, _, _>(&mut event_loop)?,
        sender::<ValveStateEvent, _, _>(&mut event_loop)?,
        timer_service.timer()?,
        sender::<ValveSpinCommandEvent, _, _>(&mut event_loop)?,
        receiver::<ValveSpinCommandEvent, _, _>(&mut event_loop)?,
        sender::<ValveSpinNotifEvent, _, _>(&mut event_loop)?,
        receiver::<ValveSpinNotifEvent, _, _>(&mut event_loop)?,
        valve_power_pin,
        valve_open_pin,
        valve_close_pin,
    );

    let powered_adc1 = adc::PoweredAdc::new(
        peripherals.adc1,
        adc::config::Config::new().calibration(true),
    )?;

    let battery_pin = peripherals.pins.gpio35.into_analog_atten_11db()?;
    let power_pin = peripherals.pins.gpio14.into_input()?;

    let battery = battery::run(
        battery_state.clone(),
        sender::<BatteryStateEvent, _, _>(&mut event_loop)?,
        timer_service.timer()?,
        powered_adc1,
        battery_pin,
        power_pin,
    );

    let mut pulse_counter = PulseCounter::new(peripherals.ulp);

    pulse_counter.initialize()?;

    let water_meter = water_meter::run(
        water_meter_state.clone(),
        receiver::<WaterMeterCommandEvent, _, _>(&mut event_loop)?,
        sender::<WaterMeterStateEvent, _, _>(&mut event_loop)?,
        timer_service.timer()?,
        pulse_counter,
    );

    let mqttconf = MqttClientConfiguration {
        client_id: Some("water-meter-demo"),
        ..Default::default()
    };

    let (mut mqttc, mqttconn) = EspMqttClient::new_async("mqtt://broker.emqx.io:1883", &mqttconf)?;

    let topic_prefix = "water-meter-demo";

    mqtt_recv::subscribe(&mut mqttc, topic_prefix);

    let mqtt_receiver = mqtt_recv::run(
        mqttconn,
        sender::<MqttClientNotificationEvent, _, _>(&mut event_loop)?,
        sender::<MqttCommandEvent, _, _>(&mut event_loop)?,
    );

    let mqtt_sender = mqtt_send::run(
        mqttc,
        sender::<MqttPublishNotificationEvent, _, _>(&mut event_loop)?,
        topic_prefix,
        receiver::<ValveStateEvent, _, _>(&mut event_loop)?,
        receiver::<WaterMeterStateEvent, _, _>(&mut event_loop)?,
        receiver::<BatteryStateEvent, _, _>(&mut event_loop)?,
    );

    let emergency = emergency::run(
        sender::<ValveCommandEvent, _, _>(&mut event_loop)?,
        receiver::<WaterMeterStateEvent, _, _>(&mut event_loop)?,
        receiver::<BatteryStateEvent, _, _>(&mut event_loop)?,
    );

    esp_idf_sys::esp!(unsafe {
        esp_idf_sys::esp_vfs_eventfd_register(&esp_idf_sys::esp_vfs_eventfd_config_t {
            max_fds: 5,
            ..Default::default()
        })
    })?;

    smol::block_on(async move {
        join(
            join(join(battery, water_meter), join(mqtt_sender, mqtt_receiver)),
            join(join(valve, emergency), join(wifi_notif, event_logger)),
        )
        .await
    });

    Ok(())
}
