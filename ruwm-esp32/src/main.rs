use esp_idf_hal::{mutex::Mutex, prelude::Peripherals};
use esp_idf_svc::eventloop::EspExplicitEventLoop;
use ruwm::{
    state_snapshot::StateSnapshot,
    valve::{self, ValveState},
};

mod event;

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

fn main() -> anyhow::Result<()> {
    esp_idf_sys::link_patches();

    let peripherals = Peripherals::take().unwrap();

    let event_loop = EspExplicitEventLoop::new(&Default::default())?;

    let valve_state = StateSnapshot::<Mutex<Option<ValveState>>>::new();

    // let valve = valve::run(

    //     Rc::new(RefCell::new(Valve::new(
    //     &bus_timer_service,
    //     event_bus.postbox()?,
    //     event_bus.postbox()?,
    //     ValveStateStorage,
    // )?));

    // let water_meter = Rc::new(RefCell::new(WaterMeter::new(
    //     &bus_timer_service,
    //     event_bus.postbox()?,
    //     event_bus.postbox()?,
    //     EspSystemTime,
    //     pulse_counter::PulseCounter::new(peripherals.ulp),
    //     WaterMeterStorage,
    // )?));

    // let powered_adc1 = adc::PoweredAdc::new(
    //     peripherals.adc1,
    //     adc::config::Config::new().calibration(true),
    // )?;

    // let battery_pin = peripherals.pins.gpio32.into_analog_atten_11db()?;
    // let power_pin = peripherals.pins.gpio2.into_input()?;

    // let battery = Rc::new(RefCell::new(Battery::new(
    //     &bus_timer_service,
    //     event_bus.postbox()?,
    //     BatteryStateStorage,
    //     powered_adc1,
    //     battery_pin,
    //     power_pin,
    // )?));

    // let mut mqtt_client_notifications = MqttClientNotifications::new(event_bus.postbox()?)?;
    // let mut mqtt_commands = MqttCommands::new::<EspMessage>(event_bus.postbox()?)?;

    // let mut mqttc = EspMqttClient::new_with_callback(
    //     "mqtt://foo",
    //     &Default::default(),
    //     move |event| {
    //         mqtt_client_notifications
    //             .process(&event)
    //             .and_then(|_| mqtt_commands.process(&event))
    //         .unwrap();
    //     },
    // )?;

    // MqttCommands::<EspPostbox>::subscribe(&mut mqttc, None)?;

    // let mqtt_updates = MqttStatusUpdates::new(
    //     mqttc,
    //     &event_bus,
    //     valve.clone(),
    //     water_meter.clone(),
    //     battery.clone(),
    // )?;

    // event_bus.spin(None)?;

    println!("Hello, world!");

    // std::thread::_SC_SPAWN
    Ok(())
}
