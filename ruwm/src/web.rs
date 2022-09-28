use core::cell::Cell;
use core::fmt::Debug;

use log::info;

use postcard::{from_bytes, to_slice};

use embassy_futures::select::select4;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::mutex::Mutex as AsyncMutex;

use embedded_svc::ws::asynch::server::Acceptor;
use embedded_svc::ws::{self, FrameType};

use edge_frame::dto::Role;

use crate::battery::BatteryState;
use crate::channel::{Receiver, Sender};
use crate::error::EitherError;
use crate::notification::Notification;
use crate::state::StateCellRead;
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};

pub use crate::dto::web::*;

#[derive(Debug)]
enum WebFrame {
    Request(WebRequest),
    Control,
    Close,
    Unknown,
}

pub const WS_MAX_FRAME_LEN: usize = 512;

pub struct Web<const N: usize> {
    valve_state_signals: [Notification; N],
    wm_state_signals: [Notification; N],
    wm_stats_state_signals: [Notification; N],
    battery_state_signals: [Notification; N],
}

impl<const N: usize> Web<N> {
    pub fn new() -> Self {
        Self {
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

    pub async fn process<A: Acceptor, R: RawMutex, const W: usize>(
        &'static self,
        acceptor: A,
        valve_command: impl Sender<Data = ValveCommand>,
        wm_command: impl Sender<Data = WaterMeterCommand>,
        valve_state: &'static (impl StateCellRead<Data = Option<ValveState>> + Send + Sync + 'static),
        wm_state: &'static (impl StateCellRead<Data = WaterMeterState> + Send + Sync + 'static),
        battery_state: &'static (impl StateCellRead<Data = BatteryState> + Send + Sync + 'static),
    ) {
        let valve_command = AsyncMutex::<R, _>::new(valve_command);
        let wm_command = AsyncMutex::<R, _>::new(wm_command);

        info!("Creating queue for {} workers", W);
        let channel = embassy_sync::channel::Channel::<R, _, W>::new();

        let mut workers = heapless::Vec::<_, N>::new();

        for index in 0..N {
            let channel = &channel;

            workers
                .push({
                    let valve_command = &valve_command;
                    let wm_command = &wm_command;

                    async move {
                        loop {
                            let (sender, receiver) = channel.recv().await;

                            handle_connection::<R>(
                                sender,
                                receiver,
                                (&self.valve_state_signals[index], valve_state),
                                (&self.wm_state_signals[index], wm_state),
                                (&self.battery_state_signals[index], battery_state),
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

        let workers = workers.into_array::<N>().unwrap_or_else(|_| unreachable!());

        embassy_futures::select::select(
            async {
                loop {
                    info!("Acceptor: waiting for new connection");

                    match acceptor.accept().await {
                        Ok((sender, receiver)) => {
                            info!("Acceptor: got new connection");
                            channel.send((sender, receiver)).await;
                            info!("Acceptor: connection sent");
                        }
                        Err(e) => {
                            info!("Got error when accepting a new connection: {:?}", e);
                        }
                    }
                }
            },
            embassy_futures::select::select_array(workers),
        )
        .await;

        info!("Server processing loop quit");
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

pub async fn handle_connection<R: RawMutex>(
    sender: impl ws::asynch::Sender,
    receiver: impl ws::asynch::Receiver,
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
    let sender = AsyncMutex::new(sender);

    let role = Mutex::<R, _>::new(Cell::new(Role::None));

    select4(
        receive(
            receiver,
            &sender,
            &role,
            valve_command,
            wm_command,
            valve,
            wm,
            battery,
        ),
        send_state(&sender, &role, valve_state, |state| {
            WebEvent::ValveState(state)
        }),
        send_state(&sender, &role, wm_state, |state| {
            WebEvent::WaterMeterState(state)
        }),
        send_state(&sender, &role, battery_state, |state| {
            WebEvent::BatteryState(state)
        }),
    )
    .await;

    Ok(())
}

async fn receive<R: RawMutex>(
    mut receiver: impl ws::asynch::Receiver,
    sender: &AsyncMutex<R, impl ws::asynch::Sender>,
    role: &Mutex<R, Cell<Role>>,
    mut valve_command: impl Sender<Data = ValveCommand>,
    mut wm_command: impl Sender<Data = WaterMeterCommand>,
    valve: &impl StateCellRead<Data = Option<ValveState>>,
    wm: &impl StateCellRead<Data = WaterMeterState>,
    battery: &impl StateCellRead<Data = BatteryState>,
) -> Result<(), ()> //WrapError<impl Debug>>
{
    loop {
        let request = match web_receive(&mut receiver).await.unwrap() {
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

        web_send(&mut *sender.lock().await, &web_event)
            .await
            .unwrap();
    }

    Ok(())
}

async fn send_state<R: RawMutex, T>(
    connection: &AsyncMutex<R, impl ws::asynch::Sender>,
    role: &Mutex<R, Cell<Role>>,
    mut state: impl Receiver<Data = T>,
    to_web_event: impl Fn(T) -> WebEvent,
) -> Result<(), ()> //WrapError<impl Debug>>
{
    loop {
        let state = state.recv().await;

        web_send_auth(
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

async fn web_send_auth<S>(
    ws_sender: S,
    event: &WebEvent,
    role: Role,
) -> Result<(), EitherError<S::Error, postcard::Error>>
where
    S: ws::asynch::Sender,
{
    if event.role() >= role {
        web_send(ws_sender, event).await
    } else {
        Ok(())
    }
}

async fn web_send<S>(
    mut ws_sender: S,
    event: &WebEvent,
) -> Result<(), EitherError<S::Error, postcard::Error>>
where
    S: ws::asynch::Sender,
{
    info!("[WS SEND] {:?}", event);

    let mut frame_buf = [0_u8; WS_MAX_FRAME_LEN];

    let (frame_type, size) = to_ws_frame(event, &mut frame_buf).map_err(EitherError::E2)?;

    ws_sender
        .send(frame_type, &frame_buf[..size])
        .await
        .map_err(EitherError::E1)?;

    Ok(())
}

async fn web_receive<R>(mut ws_receiver: R) -> Result<WebFrame, R::Error>
where
    R: ws::asynch::Receiver,
{
    let mut frame_buf = [0_u8; WS_MAX_FRAME_LEN];

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
