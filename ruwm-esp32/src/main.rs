use embedded_svc::{
    event_bus::nonblocking::EventBus as _,
    utils::nonblocking::{
        event_bus::{EventBus, Postbox},
        timer::{Once, Periodic},
    },
};
use esp_idf_hal::{
    adc,
    mutex::{Condvar, Mutex},
    prelude::Peripherals,
};
use esp_idf_svc::{
    eventloop::{EspExplicitEventLoop, EspTypedEventLoop},
    timer::{EspOnce, EspPeriodic},
};
use event::{WaterMeterCommandEvent, WaterMeterStateEvent};
use pulse_counter::PulseCounter;
use ruwm::{
    battery::{self, BatteryState},
    pulse_counter::PulseCounter as _,
    state_snapshot::StateSnapshot,
    valve::{self, ValveState},
    water_meter::{self, WaterMeterState},
};

use crate::event::BatteryStateEvent;

mod event;

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

fn main() -> anyhow::Result<()> {
    esp_idf_sys::link_patches();

    let peripherals = Peripherals::take().unwrap();

    let esp_event_loop = EspExplicitEventLoop::new(&Default::default())?;

    let mut periodic = Periodic::<Mutex<_>, _>::new(EspPeriodic::new()?);

    // TODO: let once = Once::<Mutex<_>, _>::new(EspOnce::new()?);

    //let postbox = Postbox::<_, Event>::new(event_loop.clone());

    //let event_bus = EventBus::<_, Condvar>::new::<Event>(event_loop);

    let valve_state = StateSnapshot::<Mutex<Option<ValveState>>>::new();

    let battery_state = StateSnapshot::<Mutex<BatteryState>>::new();

    let water_meter_state = StateSnapshot::<Mutex<WaterMeterState>>::new();

    let powered_adc1 = adc::PoweredAdc::new(
        peripherals.adc1,
        adc::config::Config::new().calibration(true),
    )?;

    let battery_pin = peripherals.pins.gpio32.into_analog_atten_11db()?;
    let power_pin = peripherals.pins.gpio2.into_input()?;

    let battery = battery::run(
        battery_state,
        Postbox::new(EspTypedEventLoop::<BatteryStateEvent, _, _>::new(
            esp_event_loop.clone(),
        )),
        battery::timer(&mut periodic),
        powered_adc1,
        battery_pin,
        power_pin,
    );

    let mut pulse_counter = PulseCounter::new(peripherals.ulp);

    pulse_counter.initialize()?;

    let water_meter = water_meter::run(
        water_meter_state,
        EventBus::<Condvar, _>::new(EspTypedEventLoop::<WaterMeterCommandEvent, _, _>::new(
            esp_event_loop.clone(),
        ))
        .subscribe()?,
        Postbox::new(EspTypedEventLoop::<WaterMeterStateEvent, _, _>::new(
            esp_event_loop.clone(),
        )),
        water_meter::timer(&mut periodic),
        pulse_counter,
    );

    // let valve = valve::run(

    //     Rc::new(RefCell::new(Valve::new(
    //     &bus_timer_service,
    //     event_bus.postbox()?,
    //     event_bus.postbox()?,
    //     ValveStateStorage,
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
