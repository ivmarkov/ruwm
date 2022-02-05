use core::cell::RefCell;

extern crate alloc;
use alloc::rc::Rc;

#[cfg(any(esp32, esp32s2))]
mod pulse_counter;

// pub struct WaterMeterStorage;

// impl Storage<WaterMeterStats> for WaterMeterStorage {
//     fn get(&self) -> WaterMeterStats {
//         todo!()
//     }

//     fn set(&mut self, data: WaterMeterStats) {
//         todo!()
//     }
// }

// pub struct ValveStateStorage;

// impl Storage<Option<ValveState>> for ValveStateStorage {
//     fn get(&self) -> Option<ValveState> {
//         todo!()
//     }

//     fn set(&mut self, data: Option<ValveState>) {
//         todo!()
//     }
// }

// pub struct BatteryStateStorage;

// impl Storage<BatteryState> for BatteryStateStorage {
//     fn get(&self) -> BatteryState {
//         todo!()
//     }

//     fn set(&mut self, data: BatteryState) {
//         todo!()
//     }
// }

fn main() -> anyhow::Result<()> {
    esp_idf_sys::link_patches();

    // let peripherals = Peripherals::take().unwrap();

    // let event_loop = EspPinnedEventLoop::new(&Default::default())?;
    // let bus_timer_service = utils::pinned_timer::PinnedTimerService::new(
    //     EspTimerService::new()?,
    //     &event_bus,
    //     event_bus.postbox()?,
    // )?;

    // let sys_time = EspSystemTime;

    // let water_meter = Rc::new(RefCell::new(WaterMeter::new(
    //     &bus_timer_service,
    //     event_bus.postbox()?,
    //     event_bus.postbox()?,
    //     EspSystemTime,
    //     pulse_counter::PulseCounter::new(peripherals.ulp),
    //     WaterMeterStorage,
    // )?));

    // let valve = Rc::new(RefCell::new(Valve::new(
    //     &bus_timer_service,
    //     event_bus.postbox()?,
    //     event_bus.postbox()?,
    //     ValveStateStorage,
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
