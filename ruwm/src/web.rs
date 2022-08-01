use core::cell::Cell;
use core::fmt::Debug;

use log::info;
use postcard::{from_bytes, to_slice};

use embassy_util::blocking_mutex::raw::RawMutex;
use embassy_util::blocking_mutex::Mutex;
use embassy_util::channel::mpmc::Channel;
use embassy_util::mutex::Mutex as AsyncMutex;
use embassy_util::waitqueue::MultiWakerRegistration;
use embassy_util::{select4, select_all};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::errors::wrap::EitherError;
use embedded_svc::utils::role::Role;
use embedded_svc::ws::{self, FrameType};

use crate::battery::BatteryState;
use crate::notification::Notification;
use crate::state::StateCellRead;
use crate::utils::NotifReceiver;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};
use crate::web_dto::*;

type WR = MultiWakerRegistration<4>;

#[derive(Debug)]
enum WebFrame {
    Request(WebRequest),
    Control,
    Close,
    Unknown,
}

pub struct Web<const N: usize, R, T>
where
    R: RawMutex,
{
    channel: Channel<R, T, 1>,
    valve_state_signals: [Notification; N],
    wm_state_signals: [Notification; N],
    wm_stats_state_signals: [Notification; N],
    battery_state_signals: [Notification; N],
}

impl<const N: usize, R, T> Web<N, R, T>
where
    R: RawMutex,
    T: ws::asynch::Receiver + ws::asynch::Sender,
{
    pub fn new() -> Self {
        Self {
            channel: Channel::new(),
            valve_state_signals: Self::notif_arr(),
            wm_state_signals: Self::notif_arr(),
            wm_stats_state_signals: Self::notif_arr(),
            battery_state_signals: Self::notif_arr(),
        }
    }

    pub fn valve_state_sinks(&self) -> [&Notification; N] {
        Self::as_refs_notif_arr(&self.valve_state_signals)
    }

    pub fn wm_state_sinks(&self) -> [&Notification; N] {
        Self::as_refs_notif_arr(&self.wm_state_signals)
    }

    pub fn wm_stats_state_sinks(&self) -> [&Notification; N] {
        Self::as_refs_notif_arr(&self.wm_stats_state_signals)
    }

    pub fn battery_state_sinks(&self) -> [&Notification; N] {
        Self::as_refs_notif_arr(&self.battery_state_signals)
    }

    pub async fn handle(&self, connection: T) {
        self.channel.send(connection).await
    }

    pub async fn process<const F: usize>(
        &'static self,
        valve_command: impl Sender<Data = ValveCommand>,
        wm_command: impl Sender<Data = WaterMeterCommand>,
        valve_state: &'static (impl StateCellRead<Data = Option<ValveState>> + Send + Sync + 'static),
        wm_state: &'static (impl StateCellRead<Data = WaterMeterState> + Send + Sync + 'static),
        battery_state: &'static (impl StateCellRead<Data = BatteryState> + Send + Sync + 'static),
    ) {
        let valve_command = AsyncMutex::<R, _>::new(valve_command);
        let wm_command = AsyncMutex::<R, _>::new(wm_command);

        let mut workers = heapless::Vec::<_, N>::new();

        for index in 0..N {
            workers
                .push({
                    let valve_command = &valve_command;
                    let wm_command = &wm_command;

                    async move {
                        loop {
                            let connection = self.channel.recv().await;

                            handle_connection::<R, F>(
                                connection,
                                NotifReceiver::new(&self.valve_state_signals[index], valve_state),
                                NotifReceiver::new(&self.wm_state_signals[index], wm_state),
                                NotifReceiver::new(
                                    &self.battery_state_signals[index],
                                    battery_state,
                                ),
                                &mut *valve_command.lock().await,
                                &mut *wm_command.lock().await,
                                valve_state,
                                wm_state,
                                battery_state,
                            )
                            .await
                            .unwrap(); // TODO
                        }
                    }
                })
                .map_err(|_| ())
                .unwrap();
        }

        select_all(workers.into_array::<N>().unwrap_or_else(|_| unreachable!())).await;
    }

    fn as_refs_notif_arr(arr: &[Notification; N]) -> [&Notification; N] {
        arr.iter()
            .collect::<heapless::Vec<_, N>>()
            .into_array::<N>()
            .unwrap_or_else(|_| unreachable!())
    }

    fn notif_arr() -> [Notification; N] {
        (0..N)
            .into_iter()
            .map(|_| Notification::new())
            .collect::<heapless::Vec<_, N>>()
            .into_array()
            .unwrap_or_else(|_| unreachable!())
    }
}

pub async fn handle_connection<R: RawMutex, const F: usize>(
    connection: impl ws::asynch::Sender + ws::asynch::Receiver,
    valve_state: impl Receiver<Data = Option<ValveState>>,
    wm_state: impl Receiver<Data = WaterMeterState>,
    battery_state: impl Receiver<Data = BatteryState>,
    valve_command: impl Sender<Data = ValveCommand>,
    wm_command: impl Sender<Data = WaterMeterCommand>,
    valve: &impl StateCellRead<Data = Option<ValveState>>,
    wm: &impl StateCellRead<Data = WaterMeterState>,
    battery: &impl StateCellRead<Data = BatteryState>,
) -> Result<(), ()> //WrapError<impl Debug>>
{
    let connection = AsyncMutex::<R, _>::new(connection);

    let role = Mutex::<R, _>::new(Cell::new(Role::None));

    select4(
        receive::<F, _>(
            &connection,
            &role,
            valve_command,
            wm_command,
            valve,
            wm,
            battery,
        ),
        send_state::<F, _, _>(&connection, &role, valve_state, |state| {
            WebEvent::ValveState(state)
        }),
        send_state::<F, _, _>(&connection, &role, wm_state, |state| {
            WebEvent::WaterMeterState(state)
        }),
        send_state::<F, _, _>(&connection, &role, battery_state, |state| {
            WebEvent::BatteryState(state)
        }),
    )
    .await;

    Ok(())
}

async fn receive<const F: usize, R: RawMutex>(
    connection: &AsyncMutex<R, impl ws::asynch::Sender + ws::asynch::Receiver>,
    role: &Mutex<R, Cell<Role>>,
    mut valve_command: impl Sender<Data = ValveCommand>,
    mut wm_command: impl Sender<Data = WaterMeterCommand>,
    valve: &impl StateCellRead<Data = Option<ValveState>>,
    wm: &impl StateCellRead<Data = WaterMeterState>,
    battery: &impl StateCellRead<Data = BatteryState>,
) -> Result<(), ()> //WrapError<impl Debug>>
{
    loop {
        let request = match web_receive::<F, _>(&mut *connection.lock().await)
            .await
            .unwrap()
        {
            WebFrame::Request(request) => request,
            WebFrame::Control => todo!(),
            WebFrame::Close => break,
            WebFrame::Unknown => return Err(()),
        };

        let response = request.response(role.lock(|role| role.get()));

        let web_event = if response.is_accepted() {
            match request.payload() {
                WebRequestPayload::ValveCommand(command) => {
                    valve_command.send(*command).await;
                    WebEvent::Response(response)
                }
                WebRequestPayload::WaterMeterCommand(command) => {
                    wm_command.send(*command).await;
                    WebEvent::Response(response)
                }
                WebRequestPayload::Authenticate(username, password) => {
                    if let Some(new_role) = authenticate(username, password) {
                        info!("[WS] Authenticated; role: {}", new_role);

                        role.lock(|role| role.set(new_role));
                        WebEvent::RoleState(new_role)
                    } else {
                        info!("[WS] Authentication failed");

                        role.lock(|role| role.set(Role::None));
                        WebEvent::AuthenticationFailed
                    }
                }
                WebRequestPayload::Logout => {
                    role.lock(|role| role.set(Role::None));
                    WebEvent::RoleState(Role::None)
                }
                WebRequestPayload::ValveStateRequest => WebEvent::ValveState(valve.get()),
                WebRequestPayload::WaterMeterStateRequest => WebEvent::WaterMeterState(wm.get()),
                WebRequestPayload::BatteryStateRequest => WebEvent::BatteryState(battery.get()),
                WebRequestPayload::WifiStatusRequest => todo!(),
            }
        } else {
            WebEvent::Response(response)
        };

        web_send::<F, _>(&mut *connection.lock().await, &web_event)
            .await
            .unwrap();
    }

    Ok(())
}

async fn send_state<const F: usize, R: RawMutex, T>(
    connection: &AsyncMutex<R, impl ws::asynch::Sender + ws::asynch::Receiver>,
    role: &Mutex<R, Cell<Role>>,
    mut state: impl Receiver<Data = T>,
    to_web_event: impl Fn(T) -> WebEvent,
) -> Result<(), ()> //WrapError<impl Debug>>
{
    loop {
        let state = state.recv().await;

        web_send_auth::<F, _>(
            &mut *connection.lock().await,
            &to_web_event(state),
            role.lock(|role| role.get()),
        )
        .await
        .unwrap();
    }
}

fn authenticate(_username: &str, _password: &str) -> Option<Role> {
    Some(Role::Admin) // TODO
}

async fn web_send_auth<const F: usize, S>(
    ws_sender: S,
    event: &WebEvent,
    role: Role,
) -> Result<(), EitherError<S::Error, postcard::Error>>
where
    S: ws::asynch::Sender,
{
    if event.role() >= role {
        web_send::<F, _>(ws_sender, event).await
    } else {
        Ok(())
    }
}

async fn web_send<const F: usize, S>(
    mut ws_sender: S,
    event: &WebEvent,
) -> Result<(), EitherError<S::Error, postcard::Error>>
where
    S: ws::asynch::Sender,
{
    info!("[WS SEND] {:?}", event);

    let mut frame_buf = [0_u8; F];

    let (frame_type, size) = to_ws_frame(event, &mut frame_buf).map_err(EitherError::E2)?;

    ws_sender
        .send(frame_type, &frame_buf[..size])
        .await
        .map_err(EitherError::E1)?;

    Ok(())
}

async fn web_receive<const F: usize, R>(mut ws_receiver: R) -> Result<WebFrame, R::Error>
where
    R: ws::asynch::Receiver,
{
    let mut frame_buf = [0_u8; F];

    let (frame_type, size) = ws_receiver.recv(&mut frame_buf).await?;

    let receive = from_ws_frame(frame_type, &frame_buf[..size]);

    info!("[WS RECEIVE] {:?}", receive);

    Ok(receive)
}

fn from_ws_frame(frame_type: FrameType, frame_buf: &[u8]) -> WebFrame {
    if frame_type.is_fragmented() {
        WebFrame::Unknown
    } else {
        match frame_type {
            FrameType::Text(_) | FrameType::Continue(_) => WebFrame::Unknown,
            FrameType::Binary(_) => {
                from_bytes(frame_buf).map_or_else(|_| WebFrame::Unknown, WebFrame::Request)
            }
            FrameType::Ping | FrameType::Pong => WebFrame::Control,
            FrameType::Close | FrameType::SocketClose => WebFrame::Close,
        }
    }
}

fn to_ws_frame(
    event: &WebEvent,
    frame_buf: &mut [u8],
) -> Result<(FrameType, usize), postcard::Error> {
    let slice = to_slice(event, frame_buf)?;

    Ok((FrameType::Binary(false), slice.len()))
}
