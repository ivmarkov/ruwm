use core::fmt::Debug;
use core::time::Duration;

use serde::{Deserialize, Serialize};

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::asyncs::Sender;
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::timer::asyncs::OnceTimer;

use crate::error;
use crate::state_snapshot::StateSnapshot;

const ROUND_UP: u16 = 50; // TODO: Make it smaller once ADC is connected

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
    
    pub async fn run<ADC, BP>(
        &self, 
        notif: impl Sender<Data = BatteryState>,
        timer: impl OnceTimer,
        one_shot: impl adc::OneShot<ADC, u16, BP>,
        battery_pin: BP,
        power_pin: impl InputPin<Error = impl error::HalError>,
    ) -> error::Result<()>
    where
        BP: adc::Channel<ADC>,
    {
        run(
            &self.state,
            notif,
            timer,
            one_shot,
            battery_pin,
            power_pin,
        ).await
    }
}

pub async fn run<ADC, BP>(
    state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
    mut notif: impl Sender<Data = BatteryState>,
    mut timer: impl OnceTimer,
    mut one_shot: impl adc::OneShot<ADC, u16, BP>,
    mut battery_pin: BP,
    power_pin: impl InputPin<Error = impl error::HalError>,
) -> error::Result<()>
where
    BP: adc::Channel<ADC>,
{
    loop {
        timer
            .after(Duration::from_secs(2))
            .map_err(error::svc)?
            .await
            .map_err(error::svc)?;

        let voltage = Some(100);
        // let voltage = one_shot
        //     .read(&mut battery_pin)
        //     .ok()
        //     .map(|voltage| voltage / ROUND_UP * ROUND_UP);
        //.map_err(error::wrap_display)?; TODO

        let powered = Some(power_pin.is_high().map_err(error::hal)?);

        state
            .update_with(
                |state| {
                    Ok(BatteryState {
                        prev_voltage: state.voltage,
                        prev_powered: state.powered,
                        voltage,
                        powered,
                    })
                },
                &mut notif,
            )
            .await?;
    }
}
