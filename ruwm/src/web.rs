use core::fmt::Debug;
use core::mem;

use embedded_svc::utils::asynch::channel::adapt;
use embedded_svc::utils::asynch::signal::AtomicSignal;
use enumset::{EnumSet, EnumSetType};
use log::info;
use postcard::{from_bytes, to_slice};

use embedded_svc::channel::asynch::{Receiver, Sender};
use embedded_svc::errors::wrap::{EitherError, WrapError};
use embedded_svc::mutex::{Mutex, MutexFamily};
use embedded_svc::utils::asynch::select::{select, select4, select_all_hvec, Either, Either4};
use embedded_svc::utils::asynch::signal::adapt::as_channel;
use embedded_svc::utils::role::Role;
use embedded_svc::ws::asynch::{Acceptor, Receiver as _, Sender as _};
use embedded_svc::ws::FrameType;

use crate::battery::BatteryState;
use crate::state::StateCellRead;
use crate::utils::{as_static_receiver, as_static_sender};
use crate::valve::{ValveCommand, ValveState};
use crate::water_meter::{WaterMeterCommand, WaterMeterState};
use crate::web_dto::*;

pub type ConnectionId = usize;

pub struct SenderInfo<A: Acceptor> {
    id: ConnectionId,
    role: Role,
    pending_responses: EnumSet<ResponseType>,
    sender: Option<A::Sender>,
}

#[derive(Debug, EnumSetType)]
enum ResponseType {
    AuthFailed,
    Role,
    WifiStatus,
    // WifiConf,
    // MqttStatus,
    // MqttConf,
    Valve,
    WM,
    Battery,
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
    M: MutexFamily,
    A: Acceptor,
{
    connections: M::Mutex<heapless::Vec<SenderInfo<A>, N>>,
    pending_responses_signal: AtomicSignal<()>,
    valve_state_signal: AtomicSignal<()>,
    wm_state_signal: AtomicSignal<()>,
    wm_stats_state_signal: AtomicSignal<()>,
    battery_state_signal: AtomicSignal<()>,
}

impl<M, A, const N: usize> Web<M, A, N>
where
    M: MutexFamily,
    A: Acceptor,
{
    pub fn new() -> Self {
        Self {
            connections: M::Mutex::new(heapless::Vec::<_, N>::new()),
            pending_responses_signal: AtomicSignal::new(),
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

    pub async fn send<const F: usize>(
        &'static self,
        valve_state: &(impl StateCellRead<Data = Option<ValveState>> + Sync),
        wm_state: &(impl StateCellRead<Data = WaterMeterState> + Sync),
        battery_state: &(impl StateCellRead<Data = BatteryState> + Sync),
    ) {
        send::<A, N, F>(
            &self.connections,
            as_static_receiver(&self.pending_responses_signal),
            adapt::adapt(as_static_receiver(&self.valve_state_signal), |_| {
                Some(valve_state.get())
            }),
            adapt::adapt(as_static_receiver(&self.wm_state_signal), |_| {
                Some(wm_state.get())
            }),
            adapt::adapt(as_static_receiver(&self.battery_state_signal), |_| {
                Some(battery_state.get())
            }),
            valve_state,
            wm_state,
            battery_state,
        )
        .await
        .unwrap(); // TODO
    }

    pub async fn receive<const F: usize>(
        &'static self,
        ws_acceptor: A,
        valve_command: impl Sender<Data = ValveCommand>,
        wm_command: impl Sender<Data = WaterMeterCommand>,
    ) {
        receive::<A, N, F>(
            &self.connections,
            ws_acceptor,
            as_static_sender(&self.pending_responses_signal),
            valve_command,
            wm_command,
        )
        .await
        .unwrap(); // TODO
    }
}

pub async fn send<A, const N: usize, const F: usize>(
    connections: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    mut pending_responses_source: impl Receiver<Data = ()>,
    mut valve_state_source: impl Receiver<Data = Option<ValveState>>,
    mut wm_state_source: impl Receiver<Data = WaterMeterState>,
    mut battery_state_source: impl Receiver<Data = BatteryState>,
    valve_state: &impl StateCellRead<Data = Option<ValveState>>,
    wm_state: &impl StateCellRead<Data = WaterMeterState>,
    battery_state: &impl StateCellRead<Data = BatteryState>,
) -> Result<(), WrapError<impl Debug>>
where
    A: Acceptor,
{
    loop {
        let pending = pending_responses_source.recv();
        let valve = valve_state_source.recv();
        let wm = wm_state_source.recv();
        let battery = battery_state_source.recv();

        //pin_mut!(pending, valve, wm, battery);

        let (response_types, pending_only) = match select4(pending, valve, wm, battery).await {
            Either4::First(_) => (EnumSet::all(), true),
            Either4::Second(_) => (EnumSet::only(ResponseType::Valve), false),
            Either4::Third(_) => (EnumSet::only(ResponseType::WM), false),
            Either4::Fourth(_) => (EnumSet::only(ResponseType::Battery), false),
        };

        for response_type in [
            ResponseType::AuthFailed,
            ResponseType::Role,
            ResponseType::WifiStatus,
            ResponseType::Valve,
            ResponseType::Battery,
            ResponseType::WM,
        ]
        // Important to first reply to auth requests which failed, then to role requests, and then to everything else
        {
            if response_types.contains(response_type) {
                let c_r = {
                    let mut connections = connections.lock();

                    let c_r = connections
                        .iter()
                        .filter(|si| !pending_only || si.pending_responses.contains(response_type))
                        .map(|si| (si.id, si.role))
                        .collect::<heapless::Vec<_, N>>();

                    for connection in &mut *connections {
                        connection.pending_responses.remove(response_type);
                    }

                    c_r
                };

                for (connection_id, role) in c_r {
                    let event = match response_type {
                        ResponseType::AuthFailed => WebEvent::AuthenticationFailed,
                        ResponseType::Role => WebEvent::RoleState(role),
                        ResponseType::WifiStatus => todo!(),
                        ResponseType::Valve => WebEvent::ValveState(valve_state.get()),
                        ResponseType::WM => WebEvent::WaterMeterState(wm_state.get()),
                        ResponseType::Battery => WebEvent::BatteryState(battery_state.get()),
                    };

                    if event.role() >= role {
                        web_send_single::<A, N, F>(connections, connection_id, &event).await?;
                    }
                }
            }
        }
    }
}

pub async fn receive<A, const N: usize, const F: usize>(
    connections: &'static impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    mut ws_acceptor: A,
    mut pending_responses_sink: impl Sender<Data = ()>,
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
                ws_acceptor
                    .accept()
                    .await
                    .map_err(EitherError::E1)?
                    .map_or_else(
                        || SelectResult::Close,
                        |(ws_sender, ws_receiver)| SelectResult::Accept(ws_sender, ws_receiver),
                    )
            } else {
                let ws_acceptor = ws_acceptor.accept();
                let ws_receivers = select_all_hvec(ws_receivers);

                //pin_mut!(ws_acceptor, ws_receivers);

                match select(ws_acceptor, ws_receivers).await {
                    Either::First(accept) => accept.map_err(EitherError::E2)?.map_or_else(
                        || SelectResult::Close,
                        |(ws_sender, ws_receiver)| SelectResult::Accept(ws_sender, ws_receiver),
                    ),
                    Either::Second((receive, _)) => {
                        let (size, frame) = receive.map_err(EitherError::E2)?;

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
                        pending_responses: EnumSet::all(),
                        sender: Some(new_sender),
                    })
                    .unwrap_or_else(|_| panic!());

                ws_receivers.push(new_receiver).unwrap_or_else(|_| panic!());

                pending_responses_sink.send(()).await;
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

                        process_request::<A, N, F>(
                            connections,
                            id,
                            role,
                            request,
                            &mut pending_responses_sink,
                            &mut valve_command_sink,
                            &mut wm_command_sink,
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
async fn process_request<A, const N: usize, const F: usize>(
    connections: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    connection_id: ConnectionId,
    role: Role,
    request: &WebRequest,
    pending_responses_sink: &mut impl Sender<Data = ()>,
    valve_command: &mut impl Sender<Data = ValveCommand>,
    wm_command: &mut impl Sender<Data = WaterMeterCommand>,
) where
    A: Acceptor,
{
    let response = request.response(role);
    let accepted = response.is_accepted();

    if accepted {
        match request.payload() {
            WebRequestPayload::ValveCommand(command) => {
                valve_command.send(*command).await;
            }
            WebRequestPayload::WaterMeterCommand(command) => {
                wm_command.send(*command).await;
            }
            other => {
                {
                    let mut connections = connections.lock();

                    let mut si = connections
                        .iter_mut()
                        .find(|si| si.id == connection_id)
                        .unwrap();

                    match other {
                        WebRequestPayload::Authenticate(username, password) => {
                            if let Some(role) = authenticate(username, password) {
                                info!("[WS] Authenticated; role: {}", role);

                                si.role = role;
                                si.pending_responses.insert(ResponseType::Role);
                            } else {
                                info!("[WS] Authentication failed");

                                si.role = Role::None;
                                si.pending_responses.insert(ResponseType::AuthFailed);
                            }
                        }
                        WebRequestPayload::Logout => {
                            si.role = Role::None;
                            si.pending_responses.insert(ResponseType::Role);
                        }
                        WebRequestPayload::ValveStateRequest => {
                            si.pending_responses.insert(ResponseType::Valve);
                        }
                        WebRequestPayload::WaterMeterStateRequest => {
                            si.pending_responses.insert(ResponseType::WM);
                        }
                        WebRequestPayload::BatteryStateRequest => {
                            si.pending_responses.insert(ResponseType::Battery);
                        }
                        WebRequestPayload::WifiStatusRequest => todo!(),
                        _ => unreachable!(),
                    }
                }

                pending_responses_sink.send(()).await;
            }
        }
    }
}

fn authenticate(_username: &str, _password: &str) -> Option<Role> {
    Some(Role::Admin) // TODO
}

async fn web_send_single<A, const N: usize, const F: usize>(
    connections: &impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
    id: ConnectionId,
    event: &WebEvent,
) -> Result<(), EitherError<A::Error, postcard::Error>>
where
    A: Acceptor,
{
    let sender = if let Some(si) = connections.lock().iter_mut().find(|si| si.id == id) {
        mem::replace(&mut si.sender, None)
    } else {
        None
    };

    if let Some(mut sender) = sender {
        let result = web_send::<A, F>(&mut sender, event).await;

        if let Some(si) = connections.lock().iter_mut().find(|si| si.id == id) {
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
