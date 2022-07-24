use core::fmt::Debug;

use embedded_svc::utils::asynch::mutex::AsyncMutex;
use embedded_svc::utils::asynch::signal::AtomicSignal;
use embedded_svc::utils::asynch::waker::MultiWakerRegistration;
use embedded_svc::utils::mutex::Mutex;
use log::info;
use postcard::{from_bytes, to_slice};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::errors::wrap::{EitherError, WrapError};
use embedded_svc::mutex::RawMutex;
use embedded_svc::utils::asynch::select::select4;
use embedded_svc::utils::asynch::signal::adapt::as_channel;
use embedded_svc::utils::role::Role;
use embedded_svc::ws;
use embedded_svc::ws::FrameType;

use crate::battery::BatteryState;
use crate::state::StateCellRead;
use crate::utils::{adapt_static_receiver, as_static_receiver, StaticRef};
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

pub struct Web {
    valve_state_signal: AtomicSignal<()>,
    wm_state_signal: AtomicSignal<()>,
    wm_stats_state_signal: AtomicSignal<()>,
    battery_state_signal: AtomicSignal<()>,
}

impl Web {
    pub fn new() -> Self {
        Self {
            valve_state_signal: AtomicSignal::new(),
            wm_state_signal: AtomicSignal::new(),
            wm_stats_state_signal: AtomicSignal::new(),
            battery_state_signal: AtomicSignal::new(),
        }
    }

    pub fn valve_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.wm_state_signal)
    }

    pub fn wm_stats_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.wm_stats_state_signal)
    }

    pub fn battery_state_sink(&'static self) -> impl Sender<Data = ()> + 'static {
        as_channel(&self.battery_state_signal)
    }

    pub async fn process<R, const F: usize>(
        &'static self,
        connection: impl ws::asynch::Sender + ws::asynch::Receiver,
        valve_command: impl Sender<Data = ValveCommand>,
        wm_command: impl Sender<Data = WaterMeterCommand>,
        valve_state: &'static (impl StateCellRead<Data = Option<ValveState>> + Send + Sync + 'static),
        wm_state: &'static (impl StateCellRead<Data = WaterMeterState> + Send + Sync + 'static),
        battery_state: &'static (impl StateCellRead<Data = BatteryState> + Send + Sync + 'static),
    ) where
        R: RawMutex,
    {
        let battery_state_wrapper = StaticRef(battery_state);
        let valve_state_wrapper = StaticRef(valve_state);
        let wm_state_wrapper = StaticRef(wm_state);

        process::<R, F>(
            connection,
            adapt_static_receiver(as_static_receiver(&self.valve_state_signal), move |_| {
                Some(valve_state_wrapper.0.get())
            }),
            adapt_static_receiver(as_static_receiver(&self.wm_state_signal), move |_| {
                Some(wm_state_wrapper.0.get())
            }),
            adapt_static_receiver(as_static_receiver(&self.battery_state_signal), move |_| {
                Some(battery_state_wrapper.0.get())
            }),
            valve_command,
            wm_command,
            valve_state,
            wm_state,
            battery_state,
        )
        .await
        .unwrap(); // TODO
    }
}

pub async fn process<R: RawMutex, const F: usize>(
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
    let connection = AsyncMutex::<R, WR, _>::new(connection);

    let role = Mutex::<R, _>::new(Role::None);

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
    connection: &AsyncMutex<R, WR, impl ws::asynch::Sender + ws::asynch::Receiver>,
    role: &Mutex<R, Role>,
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

        let response = request.response(*role.lock());

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

                        *role.lock() = new_role;
                        WebEvent::RoleState(new_role)
                    } else {
                        info!("[WS] Authentication failed");

                        *role.lock() = Role::None;
                        WebEvent::AuthenticationFailed
                    }
                }
                WebRequestPayload::Logout => {
                    *role.lock() = Role::None;
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
    connection: &AsyncMutex<R, WR, impl ws::asynch::Sender + ws::asynch::Receiver>,
    role: &Mutex<R, Role>,
    mut state: impl Receiver<Data = T>,
    to_web_event: impl Fn(T) -> WebEvent,
) -> Result<(), ()> //WrapError<impl Debug>>
{
    loop {
        let state = state.recv().await;

        web_send_auth::<F, _>(
            &mut *connection.lock().await,
            &to_web_event(state),
            *role.lock(),
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
