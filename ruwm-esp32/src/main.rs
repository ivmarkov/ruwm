use embedded_svc::{event_bus::Spin, utils};

use esp_idf_hal::prelude::Peripherals;

use esp_idf_svc::{
    eventloop::{EspPinnedEventLoop, EspPostbox},
    systime::EspSystemTime,
    timer::EspTimerService,
};
use ruwm::{
    storage::Storage,
    valve::{Valve, ValveState},
    water_meter::{WaterMeter, WaterMeterStats},
};

mod pulse_counter;

pub struct WaterMeterStorage;

impl Storage<WaterMeterStats> for WaterMeterStorage {
    fn get(&self) -> WaterMeterStats {
        todo!()
    }

    fn set(&mut self, data: WaterMeterStats) {
        todo!()
    }
}

pub struct ValveStateStorage;

impl Storage<Option<ValveState>> for ValveStateStorage {
    fn get(&self) -> Option<ValveState> {
        todo!()
    }

    fn set(&mut self, data: Option<ValveState>) {
        todo!()
    }
}

fn main() -> anyhow::Result<()> {
    // Temporary. Will disappear once ESP-IDF 4.4 is released, but for now it is necessary to call this function once,
    // or else some patches to the runtime implemented by esp-idf-sys might not link properly.
    esp_idf_sys::link_patches();

    let peripherals = Peripherals::take().unwrap();

    let event_bus = EspPinnedEventLoop::new(&Default::default())?;
    let bus_timer_service = utils::pinned_timer::PinnedTimerService::new(
        EspTimerService::new()?,
        &event_bus,
        event_bus.postbox()?,
    )?;

    let sys_time = EspSystemTime;

    let water_meter = WaterMeter::new(
        &bus_timer_service,
        event_bus.postbox()?,
        EspSystemTime,
        pulse_counter::PulseCounter::new(peripherals.ulp),
        WaterMeterStorage,
    )?;

    let valve = Valve::new(
        &bus_timer_service,
        event_bus.postbox()?,
        event_bus.postbox()?,
        ValveStateStorage,
    )?;

    // TODO
    // let battery = Battery::new(
    //     &bus_timer_service,
    //     event_bus.postbox()?,
    //     event_bus.postbox()?,
    //     ValveStateStorage,
    // )?;

    event_bus.spin(None)?;

    println!("Hello, world!");

    // std::thread::_SC_SPAWN
    Ok(())
}
