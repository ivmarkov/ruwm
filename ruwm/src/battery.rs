use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::asyncs::Sender;
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::timer::asyncs::OnceTimer;

use crate::state_snapshot::StateSnapshot;

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

pub struct Battery<M>
where
    M: MutexFamily,
{
    state: StateSnapshot<M::Mutex<BatteryState>>,
}

impl<M> Battery<M>
where
    M: MutexFamily,
{
    pub fn new() -> Self {
        Self {
            state: StateSnapshot::new(),
        }
    }

    pub fn state(&self) -> &StateSnapshot<impl Mutex<Data = BatteryState>> {
        &self.state
    }

    pub async fn process<ADC, BP>(
        &self,
        timer: impl OnceTimer,
        one_shot: impl adc::OneShot<ADC, u16, BP>,
        battery_pin: BP,
        power_pin: impl InputPin,
        state_sink: impl Sender<Data = BatteryState>,
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
    state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
    mut state_sink: impl Sender<Data = BatteryState>,
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

        state
            .update_with(|state| BatteryState { voltage, powered }, &mut state_sink)
            .await;
    }
}
