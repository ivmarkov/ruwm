use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::nonblocking::PeriodicTimer;

use crate::error;
use crate::state_snapshot::StateSnapshot;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
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

pub async fn run<ADC, BP>(
    state: StateSnapshot<impl Mutex<Data = BatteryState>>,
    mut notif: impl Sender<Data = BatteryState>,
    mut timer: impl PeriodicTimer,
    mut one_shot: impl adc::OneShot<ADC, u16, BP>,
    mut battery_pin: BP,
    power_pin: impl InputPin<Error = impl error::HalError>,
) -> error::Result<()>
where
    BP: adc::Channel<ADC>,
{
    let mut tick = timer.every(Duration::from_secs(2)).map_err(error::svc)?;

    loop {
        tick.recv().await.map_err(error::svc)?;

        let voltage = one_shot.read(&mut battery_pin).ok();
        //.map_err(error::wrap_display)?; TODO

        state
            .update_with(
                |state| {
                    Ok(BatteryState {
                        prev_voltage: state.voltage,
                        prev_powered: state.powered,
                        voltage,
                        powered: Some(power_pin.is_high().map_err(error::hal)?),
                    })
                },
                &mut notif,
            )
            .await?;
    }
}
