use core::fmt::{Debug, Display};
use core::time::Duration;

use anyhow::anyhow;

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::errors::Errors;
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::nonblocking::PeriodicTimer;

use crate::state_snapshot::StateSnapshot;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct BatteryState {
    pub prev_voltage: Option<u16>,
    pub prev_powered: Option<bool>,
    pub voltage: Option<u16>,
    pub powered: Option<bool>,
}

impl BatteryState {
    pub const LOW_VOLTAGE: u16 = 2700;
    pub const MAX_VOLTAGE: u16 = 3100;
}

pub async fn run<ADC, BP, PP>(
    state: StateSnapshot<impl Mutex<Data = BatteryState>>,
    mut notif: impl Sender<Data = BatteryState>,
    mut timer: impl PeriodicTimer,
    mut one_shot: impl adc::OneShot<ADC, u16, BP>,
    mut battery_pin: BP,
    power_pin: PP,
) -> anyhow::Result<()>
where
    BP: adc::Channel<ADC>,
    PP: InputPin,
    PP::Error: Debug,
{
    let mut tick = timer
        .every(Duration::from_secs(2))
        .map_err(|e| anyhow!(e))?;

    loop {
        tick.recv().await.map_err(|e| anyhow!(e))?;

        let voltage = one_shot.read(&mut battery_pin).ok();

        state
            .update_with(
                |state| BatteryState {
                    prev_voltage: state.voltage,
                    prev_powered: state.powered,
                    voltage,
                    powered: power_pin.is_high().ok(),
                },
                &mut notif,
            )
            .await?;
    }
}
