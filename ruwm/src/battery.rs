use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embassy_util::blocking_mutex::raw::RawMutex;

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::asynch::Sender;
use embedded_svc::timer::asynch::OnceTimer;

use crate::state::{update_with, MemoryStateCell, StateCell, StateCellRead};

const ROUND_UP: u16 = 50; // TODO: Make it smaller once ADC is connected

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BatteryState {
    pub voltage: Option<u16>,
    pub powered: Option<bool>,
}

impl BatteryState {
    pub const LOW_VOLTAGE: u16 = 2700;
    pub const MAX_VOLTAGE: u16 = 3100;
}

pub struct Battery<R>
where
    R: RawMutex,
{
    state: MemoryStateCell<R, BatteryState>,
}

impl<R> Battery<R>
where
    R: RawMutex + Send + Sync + 'static,
{
    pub fn new() -> Self {
        Self {
            state: MemoryStateCell::new(Default::default()),
        }
    }

    pub fn state(&self) -> &(impl StateCellRead<Data = BatteryState> + Send + Sync) {
        &self.state
    }

    pub async fn process<ADC, BP>(
        &self,
        timer: impl OnceTimer,
        one_shot: impl adc::OneShot<ADC, u16, BP>,
        battery_pin: BP,
        power_pin: impl InputPin,
        state_sink: impl Sender<Data = ()>,
    ) where
        BP: adc::Channel<ADC>,
    {
        process(
            timer,
            one_shot,
            battery_pin,
            power_pin,
            &self.state,
            state_sink,
        )
        .await
    }
}

pub async fn process<ADC, BP>(
    mut timer: impl OnceTimer,
    _one_shot: impl adc::OneShot<ADC, u16, BP>,
    _battery_pin: BP,
    power_pin: impl InputPin,
    state: &impl StateCell<Data = BatteryState>,
    mut state_sink: impl Sender<Data = ()>,
) where
    BP: adc::Channel<ADC>,
{
    loop {
        timer.after(Duration::from_secs(2)).unwrap().await;

        let voltage = Some(100);
        // let voltage = one_shot
        //     .read(&mut battery_pin)
        //     .ok()
        //     .map(|voltage| voltage / ROUND_UP * ROUND_UP);
        //.map_err(error::wrap_display)?; TODO

        let powered = Some(power_pin.is_high().unwrap_or(false));

        update_with(
            state,
            |_state| BatteryState { voltage, powered },
            &mut state_sink,
        )
        .await;
    }
}
