use futures::pin_mut;

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::MutexFamily;
use embedded_svc::utils::asyncs::select::{select3, Either3};
use embedded_svc::utils::asyncs::signal::adapt::{as_sender, as_receiver};
use embedded_svc::utils::asyncs::signal::{MutexSignal, State};

use crate::battery::BatteryState;
use crate::error;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::WaterMeterState;

pub struct Emergency<M> 
where 
    M: MutexFamily,
{
    valve_notif: MutexSignal<M::Mutex<State<Option<ValveState>>>, Option<ValveState>>,
    wm_notif: MutexSignal<M::Mutex<State<WaterMeterState>>, WaterMeterState>,
    battery_notif: MutexSignal<M::Mutex<State<BatteryState>>, BatteryState>,
}

impl<M> Emergency<M> 
where 
    M: MutexFamily,
{
    pub fn new() -> Self {
        Self {
            valve_notif: MutexSignal::new(),
            wm_notif: MutexSignal::new(),
            battery_notif: MutexSignal::new(),
        }
    }

    pub fn valve_notif(&self) -> impl Sender<Data = Option<ValveState>> + '_ 
    where 
        M::Mutex<State<Option<ValveState>>>: Send + Sync, 
    {
        as_sender(&self.valve_notif)
    }

    pub fn wm_notif(&self) -> impl Sender<Data = WaterMeterState> + '_ 
    where 
        M::Mutex<State<WaterMeterState>>: Send + Sync, 
    {
        as_sender(&self.wm_notif)
    }

    pub fn battery_notif(&self) -> impl Sender<Data = BatteryState> + '_ 
    where 
        M::Mutex<State<BatteryState>>: Send + Sync, 
    {
        as_sender(&self.battery_notif)
    }
    
    pub async fn run(
        &self, 
        notif: impl Sender<Data = ValveCommand>,
    ) -> error::Result<()>
    where 
        M::Mutex<State<Option<ValveState>>>: Send + Sync, 
        M::Mutex<State<WaterMeterState>>: Send + Sync, 
        M::Mutex<State<BatteryState>>: Send + Sync, 
    {
        run(
            notif,
            as_receiver(&self.valve_notif),
            as_receiver(&self.wm_notif),
            as_receiver(&self.battery_notif),
        ).await
    }
}

pub async fn run(
    mut notif: impl Sender<Data = ValveCommand>,
    mut valve: impl Receiver<Data = Option<ValveState>>,
    mut wm: impl Receiver<Data = WaterMeterState>,
    mut battery: impl Receiver<Data = BatteryState>,
) -> error::Result<()> {
    let mut valve_state = None;

    loop {
        let valve = valve.recv();
        let wm = wm.recv();
        let battery = battery.recv();

        pin_mut!(valve, wm, battery);

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
            notif.send(ValveCommand::Close).await.map_err(error::svc)?;
        }
    }
}
