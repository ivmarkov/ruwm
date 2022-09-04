use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embassy_sync::blocking_mutex::raw::RawMutex;

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::timer::asynch::OnceTimer;

use crate::channel::Sender;
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

    pub const fn new() -> Self {
        Self {
            voltage: None,
            powered: None,
        }
    }
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
    pub const fn new() -> Self {
        Self {
            state: MemoryStateCell::new(BatteryState::new()),
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
    mut one_shot: impl adc::OneShot<ADC, u16, BP>,
    mut battery_pin: BP,
    power_pin: impl InputPin,
    state: &impl StateCell<Data = BatteryState>,
    mut state_sink: impl Sender<Data = ()>,
) where
    BP: adc::Channel<ADC>,
{
    loop {
        timer.after(Duration::from_secs(2)).unwrap().await;

        //let voltage = Some(100);
        let voltage = one_shot
            .read(&mut battery_pin)
            .ok()
            .map(|voltage| voltage / ROUND_UP * ROUND_UP);

        let powered = Some(power_pin.is_high().unwrap_or(false));

        update_with(
            "BATTERY",
            state,
            |_state| BatteryState { voltage, powered },
            &mut state_sink,
        )
        .await;
    }
}
