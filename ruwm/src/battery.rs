use core::fmt::Debug;
use core::time::Duration;

use embedded_hal::adc;
use embedded_hal::digital::v2::InputPin;

use embedded_svc::channel::nonblocking::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::timer::nonblocking::Periodic;

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

pub async fn run<M, N, P, ADC, A, BP, PP>(
    state: StateSnapshot<M>,
    mut notif: N,
    periodic: &mut P,
    mut one_shot: A,
    mut battery_pin: BP,
    power_pin: PP,
) where
    M: Mutex<Data = BatteryState>,
    N: Sender<Data = BatteryState>,
    P: Periodic,
    A: adc::OneShot<ADC, u16, BP>,
    BP: adc::Channel<ADC>,
    PP: InputPin,
    PP::Error: Debug,
{
    let mut timer = periodic.every(Duration::from_secs(2)).unwrap();

    loop {
        timer.recv().await.unwrap();

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
            .await;
    }
}
