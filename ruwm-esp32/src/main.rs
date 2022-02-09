use core::time::Duration;

use event::{ValveSpinCommandEvent, ValveSpinNotifEvent};
use futures::future::join;

use embedded_svc::event_bus::nonblocking::EventBus as _;
use embedded_svc::event_bus::Spin;

use esp_idf_hal::adc;
use esp_idf_hal::mutex::Mutex;
use esp_idf_hal::prelude::Peripherals;

use esp_idf_svc::eventloop::EspExplicitEventLoop;
use esp_idf_svc::mqtt::client::EspMqttClient;
use esp_idf_svc::timer::{EspOnce, EspPeriodic};

use pulse_counter::PulseCounter;

use ruwm::battery::{self, BatteryState};
use ruwm::emergency;
use ruwm::mqtt_recv;
use ruwm::mqtt_send;
use ruwm::pulse_counter::PulseCounter as _;
use ruwm::state_snapshot::StateSnapshot;
use ruwm::valve::{self, ValveState};
use ruwm::water_meter::{self, WaterMeterState};

use crate::event::{
    BatteryStateEvent, MqttClientNotificationEvent, MqttCommandEvent, MqttPublishEvent,
    ValveCommandEvent, ValveStateEvent, WaterMeterCommandEvent, WaterMeterStateEvent,
};

mod event;

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

fn state<S>() -> StateSnapshot<Mutex<S>>
where
    S: Send + Sync + Default,
{
    StateSnapshot::<Mutex<S>>::new()
}

fn main() -> anyhow::Result<()> {
    esp_idf_sys::link_patches();

    let peripherals = Peripherals::take().unwrap();

    let mut event_loop = EspExplicitEventLoop::new(&Default::default())?;

    let mut valve_command_bus = event_loop.clone().into_async::<ValveCommandEvent, _>();
    let mut valve_state_bus = event_loop.clone().into_async::<ValveStateEvent, _>();

    let mut valve_spin_command_bus = event_loop.clone().into_async::<ValveSpinCommandEvent, _>();
    let mut valve_spin_notif_bus = event_loop.clone().into_async::<ValveSpinNotifEvent, _>();

    let mut battery_state_bus = event_loop.clone().into_async::<BatteryStateEvent, _>();

    let mut wm_command_bus = event_loop.clone().into_async::<WaterMeterCommandEvent, _>();
    let mut wm_state_bus = event_loop.clone().into_async::<WaterMeterStateEvent, _>();

    let mut mqtt_command_bus = event_loop.clone().into_async::<MqttCommandEvent, _>();
    let mut mqtt_notif_bus = event_loop
        .clone()
        .into_async::<MqttClientNotificationEvent, _>();
    let mut mqtt_publish_bus = event_loop.clone().into_async::<MqttPublishEvent, _>();

    let mut periodic = EspPeriodic::new()?.into_async();
    let once = EspOnce::new()?.into_async();

    let valve_state = state::<Option<ValveState>>();
    let battery_state = state::<BatteryState>();
    let water_meter_state = state::<WaterMeterState>();

    let valve_power_pin = peripherals.pins.gpio10.into_output()?;
    let valve_open_pin = peripherals.pins.gpio11.into_output()?;
    let valve_close_pin = peripherals.pins.gpio12.into_output()?;

    let valve = valve::run(
        valve_state.clone(),
        valve_command_bus.subscribe()?,
        valve_state_bus.postbox()?,
        once,
        valve_spin_command_bus.postbox()?,
        valve_spin_command_bus.subscribe()?,
        valve_spin_notif_bus.postbox()?,
        valve_spin_notif_bus.subscribe()?,
        valve_power_pin,
        valve_open_pin,
        valve_close_pin,
    );

    let powered_adc1 = adc::PoweredAdc::new(
        peripherals.adc1,
        adc::config::Config::new().calibration(true),
    )?;

    let battery_pin = peripherals.pins.gpio32.into_analog_atten_11db()?;
    let power_pin = peripherals.pins.gpio2.into_input()?;

    let battery = battery::run(
        battery_state.clone(),
        battery_state_bus.postbox()?,
        battery::timer(&mut periodic),
        powered_adc1,
        battery_pin,
        power_pin,
    );

    let mut pulse_counter = PulseCounter::new(peripherals.ulp);

    pulse_counter.initialize()?;

    let water_meter = water_meter::run(
        water_meter_state.clone(),
        wm_command_bus.subscribe()?,
        wm_state_bus.postbox()?,
        water_meter::timer(&mut periodic),
        pulse_counter,
    );

    let mqttconf = Default::default();
    let (mut mqttc, mqttconn) = EspMqttClient::new_async("mqtt://foo", &mqttconf)?;

    let topic_prefix = "test_client";

    mqtt_send::subscribe(&mut mqttc, topic_prefix);

    let mqtt_sender = mqtt_send::run(
        mqttc,
        mqtt_publish_bus.postbox()?,
        topic_prefix,
        valve_state_bus.subscribe()?,
        wm_state_bus.subscribe()?,
        battery_state_bus.subscribe()?,
    );

    let mqtt_receiver = mqtt_recv::run(
        mqttconn,
        mqtt_notif_bus.postbox()?,
        mqtt_command_bus.postbox()?,
    );

    let emergency = emergency::run(
        valve_command_bus.postbox()?,
        wm_state_bus.subscribe()?,
        battery_state_bus.subscribe()?,
    );

    smol::block_on(async move {
        join(
            join(join(battery, water_meter), join(mqtt_sender, mqtt_receiver)),
            join(valve, emergency),
        )
        .await
    });

    loop {
        event_loop.spin(Some(Duration::from_millis(200)))?;
    }
}
