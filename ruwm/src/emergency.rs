use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::MutexFamily;
use embedded_svc::signal::asyncs::{SendSyncSignalFamily, Signal};
use embedded_svc::utils::asyncs::select::{select3, Either3};
use embedded_svc::utils::asyncs::signal::adapt::as_sender;

use crate::battery::BatteryState;
use crate::error;
use crate::utils::as_static_receiver;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::WaterMeterState;

pub struct Emergency<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    valve_state_signal: M::Signal<Option<ValveState>>,
    wm_state_signal: M::Signal<WaterMeterState>,
    battery_state_signal: M::Signal<BatteryState>,
}

impl<M> Emergency<M>
where
    M: MutexFamily + SendSyncSignalFamily,
{
    pub fn new() -> Self {
        Self {
            valve_state_signal: M::Signal::new(),
            wm_state_signal: M::Signal::new(),
            battery_state_signal: M::Signal::new(),
        }
    }

    pub fn valve_state_sink(&self) -> impl Sender<Data = Option<ValveState>> + '_ {
        as_sender(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&self) -> impl Sender<Data = WaterMeterState> + '_ {
        as_sender(&self.wm_state_signal)
    }

    pub fn battery_state_sink(&self) -> impl Sender<Data = BatteryState> + '_ {
        as_sender(&self.battery_state_signal)
    }

    pub async fn process(
        &'static self,
        valve_command: impl Sender<Data = ValveCommand>,
    ) -> error::Result<()> {
        process(
            as_static_receiver(&self.valve_state_signal),
            as_static_receiver(&self.wm_state_signal),
            as_static_receiver(&self.battery_state_signal),
            valve_command,
        )
        .await
    }
}

pub async fn process(
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
    mut valve_command_sink: impl Sender<Data = ValveCommand>,
) -> error::Result<()> {
    let mut valve_state = None;

    loop {
        let valve = valve_state_source.recv();
        let wm = wm_state_source.recv();
        let battery = battery_state_source.recv();

        //pin_mut!(valve, wm, battery);

        let emergency_close = match select3(valve, wm, battery).await {
            Either3::First(valve) => {
                let valve = valve.map_err(error::svc)?;

                valve_state = valve;

                false
            }
            Either3::Second(wm) => {
                let wm = wm.map_err(error::svc)?;

                wm.leaking
            }
            Either3::Third(battery) => {
                let battery = battery.map_err(error::svc)?;

                let battery_low = battery
                    .voltage
                    .map(|voltage| voltage <= BatteryState::LOW_VOLTAGE)
                    .unwrap_or(false);

                let powered = battery.powered.unwrap_or(false);

                battery_low && !powered
            }
        };

        if emergency_close
            && !matches!(
                valve_state,
                Some(ValveState::Closing) | Some(ValveState::Closed)
            )
        {
            valve_command_sink
                .send(ValveCommand::Close)
                .await
                .map_err(error::svc)?;
        }
    }
}
