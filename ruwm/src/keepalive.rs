use core::time::Duration;

use futures::pin_mut;

use serde::{Deserialize, Serialize};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::MutexFamily;
use embedded_svc::sys_time::SystemTime;
use embedded_svc::timer::asyncs::OnceTimer;
use embedded_svc::utils::asyncs::select::{select, Either};
use embedded_svc::utils::asyncs::signal::adapt::{as_sender, as_receiver};
use embedded_svc::utils::asyncs::signal::{MutexSignal, State};

use crate::error;
use crate::quit::Quit;

const TIMEOUT: Duration = Duration::from_secs(20);
const REMAINING_TIME_TRIGGER: Duration = Duration::from_secs(1);

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemainingTime {
    Indefinite,
    Duration(Duration),
}

pub struct Keepalive<M, const N: usize> 
where 
    M: MutexFamily,
{
    events: [MutexSignal<M::Mutex<State<()>>, ()>; N],
}

impl<M, const N: usize> Keepalive<M, N> 
where 
    M: MutexFamily,
{
    pub fn new() -> Self {
        Self {
            events: [(); N].iter()
                .map(|_| MutexSignal::new())
                .collect::<heapless::Vec<_, N>>()
                .into_array::<N>()
                .unwrap_or_else(|_| panic!()),
        }
    }

    pub fn events(&self) -> [impl Sender<Data = ()> + '_; N] 
    where 
        M::Mutex<State<()>>: Send + Sync, 
    {
        self.events.iter()
            .map(|signal| as_sender(signal))
            .collect::<heapless::Vec<_, N>>()
            .into_array()
            .unwrap_or_else(|_| panic!())
    }

    pub async fn run(
        &'static self, 
        timer: impl OnceTimer,
        system_time: impl SystemTime,
        notif: impl Sender<Data = RemainingTime>,
        quit: impl Sender<Data = Quit>,
    ) -> error::Result<()>
    where 
        M::Mutex<State<()>>: Send + Sync, 
    {
        run(
            self.events.iter()
                .map(|signal| as_receiver(signal))
                .collect::<heapless::Vec<_, N>>()
                .into_array::<N>()
                .unwrap_or_else(|_| panic!()),
            timer,
            system_time,
            notif,
            quit,
        ).await
    }
}

pub async fn run(
    mut event: impl Receiver<Data = ()>,
    mut timer: impl OnceTimer,
    system_time: impl SystemTime,
    mut notif: impl Sender<Data = RemainingTime>,
    mut quit: impl Sender<Data = Quit>,
) -> error::Result<()> {
    let mut quit_time = Some(system_time.now() + TIMEOUT);
    let mut quit_time_sent = None;

    loop {
        let event = event.recv();
        let tick = timer
            .after(Duration::from_secs(2) /*Duration::from_millis(500)*/)
            .map_err(error::svc)?;

        pin_mut!(event, tick);

        let result = select(event, tick).await;
        let now = system_time.now();

        if let Either::First(_) = result {
            quit_time = Some(now + TIMEOUT);

            // Payload::ValveCommand(_)
            // | Payload::ValveState(_)
            // | Payload::WaterMeterCommand(_)
            // | Payload::WaterMeterState(_)
            // | Payload::ButtonCommand(_)
            // | Payload::MqttClientNotification(_)
            // | Payload::WebResponse(_, _) => Some(now + TIMEOUT),
            // Payload::BatteryState(battery_state) => {
            //     battery_state.powered.unwrap_or(true).then(|| now + TIMEOUT)
            // }
        }

        if quit_time.map(|quit_time| now >= quit_time).unwrap_or(false) {
            quit.send(Quit).await?;
        } else if quit_time.is_some() != quit_time_sent.is_some()
            || quit_time_sent
                .map(|quit_time_sent| quit_time_sent + REMAINING_TIME_TRIGGER <= now)
                .unwrap_or(false)
        {
            quit_time_sent = Some(now);

            let remaining_time = quit_time
                .map(|quit_time| RemainingTime::Duration(quit_time - now))
                .unwrap_or(RemainingTime::Indefinite);

            notif.send(remaining_time).await?;
        }
    }
}
