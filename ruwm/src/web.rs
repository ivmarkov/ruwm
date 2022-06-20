use core::fmt::Debug;
use core::mem;

use log::info;
use postcard::{from_bytes, to_slice};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::errors::wrap::{EitherError, WrapError};
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::signal::asynch::{SendSyncSignalFamily, Signal};
use embedded_svc::utils::asynch::select::{select, select4, select_all_hvec, Either, Either4};
use embedded_svc::utils::asynch::signal::adapt::as_channel;
use embedded_svc::utils::role::Role;
use embedded_svc::ws::asynch::{Acceptor, Receiver as _, Sender as _};
use embedded_svc::ws::FrameType;

use crate::battery::BatteryState;
use crate::state_snapshot::StateSnapshot;
use crate::storage::Storage;
use crate::utils::{as_static_receiver, as_static_sender};
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};
use crate::water_meter_stats::WaterMeterStatsState;
use crate::web_dto::*;

pub type ConnectionId = usize;

pub struct SenderInfo<A: Acceptor> {
    id: ConnectionId,
    role: Role,
    sender: Option<A::Sender>,
}

#[derive(Debug)]
enum WebFrame {
    Request(WebRequest),
    Control,
    Close,
    Unknown,
}

pub struct Web<M, A, const N: usize>
where
    M: MutexFamily + SendSyncSignalFamily,
    A: Acceptor,
{
    connections: M::Mutex<heapless::Vec<SenderInfo<A>, N>>,
    conn_signal: M::Signal<(ConnectionId, WebEvent)>, // TODO: Signal not a good idea
    valve_state_signal: M::Signal<Option<ValveState>>,
    wm_state_signal: M::Signal<WaterMeterState>,
    wm_stats_state_signal: M::Signal<WaterMeterStatsState>,
    battery_state_signal: M::Signal<BatteryState>,
}

impl<M, A, const N: usize> Web<M, A, N>
where
    M: MutexFamily + SendSyncSignalFamily,
    A: Acceptor,
{
    pub fn new() -> Self {
        Self {
            connections: M::Mutex::new(heapless::Vec::<_, N>::new()),
            conn_signal: M::Signal::new(),
            valve_state_signal: M::Signal::new(),
            wm_state_signal: M::Signal::new(),
            wm_stats_state_signal: M::Signal::new(),
            battery_state_signal: M::Signal::new(),
        }
    }

    pub fn valve_state_sink(&'static self) -> impl Sender<Data = Option<ValveState>> + 'static {
        as_channel(&self.valve_state_signal)
    }

    pub fn wm_state_sink(&'static self) -> impl Sender<Data = WaterMeterState> + 'static {
        as_channel(&self.wm_state_signal)
    }

    pub fn wm_stats_state_sink(
        &'static self,
    ) -> impl Sender<Data = WaterMeterStatsState> + 'static {
        as_channel(&self.wm_stats_state_signal)
    }

    pub fn battery_state_sink(&'static self) -> impl Sender<Data = BatteryState> + 'static {
        as_channel(&self.battery_state_signal)
    }

    pub async fn send<const F: usize>(&'static self) {
        send::<A, N, F>(
            &self.connections,
            as_static_receiver(&self.conn_signal),
            as_static_receiver(&self.valve_state_signal),
            as_static_receiver(&self.wm_state_signal),
            as_static_receiver(&self.battery_state_signal),
        )
        .await
        .unwrap(); // TODO
    }

    pub async fn receive<const F: usize>(
        &'static self,
        ws_acceptor: A,
        valve_state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
        wm_state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
        battery_state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
        valve_command: impl Sender<Data = ValveCommand>,
        wm_command: impl Sender<Data = WaterMeterCommand>,
    ) {
        receive::<A, N, F>(
            &self.connections,
            ws_acceptor,
            valve_state,
            wm_state,
            battery_state,
            as_static_sender(&self.conn_signal),
            valve_command,
            wm_command,
        )
        .await
        .unwrap(); // TODO
    }
}

pub async fn send<A, const N: usize, const F: usize>(
    connections: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    mut conn_source: impl Receiver<Data = (ConnectionId, WebEvent)>,
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
) -> Result<(), WrapError<impl Debug>>
where
    A: Acceptor,
{
    loop {
        let receiver = conn_source.recv();
        let valve = valve_state_source.recv();
        let wm = wm_state_source.recv();
        let battery = battery_state_source.recv();

        //pin_mut!(receiver, valve, wm, battery);

        match select4(receiver, valve, wm, battery).await {
            Either4::First((id, event)) => {
                web_send_single::<A, N, F>(connections, id, &event).await?;
            }
            Either4::Second(state) => {
                web_send_all::<A, N, F>(connections, &WebEvent::ValveState(state)).await?
            }
            Either4::Third(state) => {
                web_send_all::<A, N, F>(connections, &WebEvent::WaterMeterState(state)).await?
            }
            Either4::Fourth(state) => {
                web_send_all::<A, N, F>(connections, &WebEvent::BatteryState(state)).await?
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn receive<A, const N: usize, const F: usize>(
    connections: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    mut ws_acceptor: A,
    valve_state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    wm_state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
    mut conn_sink: impl Sender<Data = (ConnectionId, WebEvent)>,
    mut valve_command_sink: impl Sender<Data = ValveCommand>,
    mut wm_command_sink: impl Sender<Data = WaterMeterCommand>,
) -> Result<(), WrapError<impl Debug>>
where
    A: Acceptor,
{
    let mut next_connection_id: ConnectionId = 0;
    let mut ws_receivers = heapless::Vec::<_, N>::new();

    loop {
        enum SelectResult<A: Acceptor> {
            Accept(A::Sender, A::Receiver),
            Close,
            Receive(usize, WebFrame),
        }

        let result: SelectResult<A> = {
            let ws_receivers = ws_receivers
                .iter_mut()
                .enumerate()
                .map(|(index, ws_receiver)| web_receive::<A, F>(ws_receiver, index))
                .collect::<heapless::Vec<_, N>>();

            if ws_receivers.is_empty() {
                ws_acceptor.accept().await?.map_or_else(
                    || SelectResult::Close,
                    |(ws_sender, ws_receiver)| SelectResult::Accept(ws_sender, ws_receiver),
                )
            } else {
                let ws_acceptor = ws_acceptor.accept();
                let ws_receivers = select_all_hvec(ws_receivers);

                //pin_mut!(ws_acceptor, ws_receivers);

                match select(ws_acceptor, ws_receivers).await {
                    Either::First(accept) => accept?.map_or_else(
                        || SelectResult::Close,
                        |(ws_sender, ws_receiver)| SelectResult::Accept(ws_sender, ws_receiver),
                    ),
                    Either::Second((receive, _)) => {
                        let (size, frame) = receive?;

                        SelectResult::Receive(size, frame)
                    }
                }
            }
        };

        match result {
            SelectResult::Accept(new_sender, new_receiver) => {
                info!("[WS ACCEPT]");

                let role = Role::None;

                let id = next_connection_id;
                next_connection_id += 1;

                connections
                    .lock()
                    .push(SenderInfo {
                        id,
                        role,
                        sender: Some(new_sender),
                    })
                    .unwrap_or_else(|_| panic!());

                ws_receivers.push(new_receiver).unwrap_or_else(|_| panic!());

                process_initial_response(
                    &mut conn_sink,
                    id,
                    role,
                    valve_state,
                    wm_state,
                    battery_state,
                )
                .await;
            }
            SelectResult::Close => {
                info!("[WS CLOSE]");
                break;
            }
            SelectResult::Receive(index, receive) => {
                match receive {
                    WebFrame::Request(ref request) => {
                        let (id, role) = {
                            let sender = &connections.lock()[index];

                            (sender.id, sender.role)
                        };

                        process_request(
                            connections,
                            id,
                            role,
                            request,
                            &mut conn_sink,
                            &mut valve_command_sink,
                            &mut wm_command_sink,
                            valve_state,
                            wm_state,
                            battery_state,
                        )
                        .await;
                    }
                    WebFrame::Control => (),
                    WebFrame::Close | WebFrame::Unknown => {
                        ws_receivers.swap_remove(index);
                        connections.lock().swap_remove(index);
                    }
                };
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn process_request<A, const N: usize>(
    sis: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    connection_id: ConnectionId,
    role: Role,
    request: &WebRequest,
    sender: &mut impl Sender<Data = (ConnectionId, WebEvent)>,
    valve_command: &mut impl Sender<Data = ValveCommand>,
    wm_command: &mut impl Sender<Data = WaterMeterCommand>,
    valve_state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    water_meter_state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
) where
    A: Acceptor,
{
    let response = request.response(role);
    let accepted = response.is_accepted();

    if accepted {
        match request.payload() {
            WebRequestPayload::Authenticate(username, password) => {
                if let Some(role) = authenticate(username, password) {
                    info!("[WS] Authenticated; role: {}", role);

                    sis.lock()
                        .iter_mut()
                        .find(|si| si.id == connection_id)
                        .unwrap()
                        .role = role;

                    process_initial_response(
                        sender,
                        connection_id,
                        role,
                        valve_state,
                        water_meter_state,
                        battery_state,
                    )
                    .await;
                } else {
                    info!("[WS] Authentication failed");

                    sender
                        .send((connection_id, WebEvent::AuthenticationFailed))
                        .await;
                }
            }
            WebRequestPayload::Logout => {
                sis.lock()
                    .iter_mut()
                    .find(|si| si.id == connection_id)
                    .unwrap()
                    .role = Role::None;

                sender
                    .send((connection_id, WebEvent::RoleState(Role::None)))
                    .await;
            }
            WebRequestPayload::ValveCommand(command) => {
                valve_command.send(*command).await;
            }
            WebRequestPayload::ValveStateRequest => {
                sender
                    .send((connection_id, WebEvent::ValveState(valve_state.get())))
                    .await;
            }
            WebRequestPayload::WaterMeterCommand(command) => {
                wm_command.send(*command).await;
            }
            WebRequestPayload::WaterMeterStateRequest => {
                sender
                    .send((
                        connection_id,
                        WebEvent::WaterMeterState(water_meter_state.get()),
                    ))
                    .await;
            }
            WebRequestPayload::BatteryStateRequest => {
                sender
                    .send((connection_id, WebEvent::BatteryState(battery_state.get())))
                    .await;
            }
            WebRequestPayload::WifiStatusRequest => todo!(),
        }
    }
}

async fn process_initial_response(
    sender: &mut impl Sender<Data = (ConnectionId, WebEvent)>,
    connection_id: ConnectionId,
    role: Role,
    valve_state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    water_meter_state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
) {
    let events = [
        WebEvent::RoleState(role),
        WebEvent::ValveState(valve_state.get()),
        WebEvent::WaterMeterState(water_meter_state.get()),
        WebEvent::BatteryState(battery_state.get()),
    ];

    for event in events {
        if role >= event.role() {
            sender.send((connection_id, event)).await;
        }
    }
}

fn authenticate(_username: &str, _password: &str) -> Option<Role> {
    Some(Role::Admin) // TODO
}

async fn web_send_all<A, const N: usize, const F: usize>(
    sis: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    event: &WebEvent,
) -> Result<(), EitherError<A::Error, postcard::Error>>
where
    A: Acceptor,
{
    let ids = sis
        .lock()
        .iter()
        .filter(|si| si.role >= event.role())
        .map(|si| si.id)
        .collect::<heapless::Vec<_, N>>();

    for id in ids {
        web_send_single::<A, N, F>(sis, id, event).await?;
    }

    Ok(())
}

async fn web_send_single<A, const N: usize, const F: usize>(
    sis: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    id: ConnectionId,
    event: &WebEvent,
) -> Result<(), EitherError<A::Error, postcard::Error>>
where
    A: Acceptor,
{
    let sender = if let Some(si) = sis.lock().iter_mut().find(|si| si.id == id) {
        mem::replace(&mut si.sender, None)
    } else {
        None
    };

    if let Some(mut sender) = sender {
        let result = web_send::<A, F>(&mut sender, event).await;

        if let Some(si) = sis.lock().iter_mut().find(|si| si.id == id) {
            si.sender = Some(sender);
        }

        result?;
    }

    Ok(())
}

async fn web_send<A, const F: usize>(
    ws_sender: &mut A::Sender,
    event: &WebEvent,
) -> Result<(), EitherError<A::Error, postcard::Error>>
where
    A: Acceptor,
{
    info!("[WS SEND] {:?}", event);

    let mut frame_buf = [0_u8; F];

    let (frame_type, size) = to_ws_frame(event, &mut frame_buf).map_err(EitherError::E2)?;

    ws_sender
        .send(frame_type, Some(&frame_buf[..size]))
        .await
        .map_err(EitherError::E1)?;

    Ok(())
}

async fn web_receive<A, const F: usize>(
    ws_receiver: &mut A::Receiver,
    index: usize,
) -> Result<(usize, WebFrame), A::Error>
where
    A: Acceptor,
{
    let mut frame_buf = [0_u8; F];

    let (frame_type, size) = ws_receiver.recv(&mut frame_buf).await?;

    let receive = from_ws_frame(frame_type, &frame_buf[..size]);

    info!("[WS RECEIVE] {}/{:?}", index, receive);

    Ok((index, receive))
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
