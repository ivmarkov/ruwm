use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::utils::asynch::channel::adapt;
use embedded_svc::utils::asynch::select::{select3, Either3};
use embedded_svc::utils::asynch::signal::adapt::as_channel;
use embedded_svc::utils::asynch::signal::AtomicSignal;

use crate::battery::BatteryState;
use crate::state::StateCellRead;
use crate::utils::as_static_receiver;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::WaterMeterState;

pub struct Emergency {
    valve_state_signal: AtomicSignal<()>,
    wm_state_signal: AtomicSignal<()>,
    battery_state_signal: AtomicSignal<()>,
}

impl Emergency {
    pub fn new() -> Self {
        Self {
            valve_state_signal: AtomicSignal::new(),
            wm_state_signal: AtomicSignal::new(),
            battery_state_signal: AtomicSignal::new(),
        }
    }

    pub fn valve_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.wm_state_signal)
    }

    pub fn battery_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.battery_state_signal)
    }

    pub async fn process(
        &'static self,
        valve_command: impl Sender<Data = ValveCommand>,
        valve_state: &(impl StateCellRead<Data = Option<ValveState>> + Sync),
        wm_state: &(impl StateCellRead<Data = WaterMeterState> + Sync),
        battery_state: &(impl StateCellRead<Data = BatteryState> + Sync),
    ) {
        process(
            adapt::adapt(as_static_receiver(&self.valve_state_signal), |_| {
                Some(valve_state.get())
            }),
            adapt::adapt(as_static_receiver(&self.wm_state_signal), |_| {
                Some(wm_state.get())
            }),
            adapt::adapt(as_static_receiver(&self.battery_state_signal), |_| {
                Some(battery_state.get())
            }),
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
) {
    let mut valve_state = None;

    loop {
        let valve = valve_state_source.recv();
        let wm = wm_state_source.recv();
        let battery = battery_state_source.recv();

        //pin_mut!(valve, wm, battery);

        let emergency_close = match select3(valve, wm, battery).await {
            Either3::First(valve) => {
                valve_state = valve;

                false
            }
            Either3::Second(wm) => wm.leaking,
            Either3::Third(battery) => {
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
            valve_command_sink.send(ValveCommand::Close).await;
        }
    }
}
