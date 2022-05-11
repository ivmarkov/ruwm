use core::mem;

extern crate alloc;
use alloc::sync::Arc;

use futures::{pin_mut, select, FutureExt};

use postcard::{from_bytes, to_slice};

use embedded_svc::channel::asyncs::{Receiver, Sender};
use embedded_svc::mutex::Mutex;
use embedded_svc::utils::asyncs::select::select_all_hvec;
use embedded_svc::utils::role::Role;
use embedded_svc::ws::asyncs::{Acceptor, Receiver as _, Sender as _};
use embedded_svc::ws::FrameType;

use crate::{
    battery::BatteryState,
    error,
    state_snapshot::StateSnapshot,
    storage::Storage,
    valve::{ValveCommand, ValveState},
    water_meter::{WaterMeterCommand, WaterMeterState},
    web_dto::*,
};

pub type ConnectionId = usize;

pub struct SenderInfo<A: Acceptor> {
    id: ConnectionId,
    role: Role,
    sender: Option<A::Sender>,
}

enum WebFrame {
    Request(WebRequest),
    Control,
    Close,
    Unknown,
}

pub fn sis<A, M, const N: usize>() -> Arc<M>
where
    A: Acceptor,
    M: Mutex<Data = heapless::Vec<SenderInfo<A>, N>>,
{
    Arc::new(M::new(heapless::Vec::<_, N>::new()))
}

#[allow(clippy::too_many_arguments)]
pub async fn run_receiver<A, const N: usize>(
    sis: Arc<impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>>,
    mut ws_acceptor: A,
    mut sender: impl Sender<Data = (ConnectionId, WebEvent)>,
    mut valve_command: impl Sender<Data = ValveCommand>,
    mut wm_command: impl Sender<Data = WaterMeterCommand>,
    valve_state: StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    water_meter_state: StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: StateSnapshot<impl Mutex<Data = BatteryState>>,
) -> error::Result<()>
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
                .map(|(index, ws_receiver)| web_receive::<A>(ws_receiver, index))
                .collect::<heapless::Vec<_, N>>();

            if ws_receivers.is_empty() {
                ws_acceptor.accept().await.map_err(error::svc)?.map_or_else(
                    || SelectResult::Close,
                    |(ws_sender, ws_receiver)| SelectResult::Accept(ws_sender, ws_receiver),
                )
            } else {
                let ws_acceptor = ws_acceptor.accept().fuse();
                let ws_receivers = select_all_hvec(ws_receivers).fuse();

                pin_mut!(ws_acceptor, ws_receivers);

                select! {
                    accept = ws_acceptor => accept
                        .map_err(error::svc)?
                        .map_or_else(|| SelectResult::Close, |(ws_sender, ws_receiver)| SelectResult::Accept(ws_sender, ws_receiver)),
                    (ws_receive, _) = ws_receivers => ws_receive.map(|(size, frame)| SelectResult::Receive(size, frame))?,
                }
            }
        };

        match result {
            SelectResult::Accept(new_sender, new_receiver) => {
                let role = Role::None;

                let id = next_connection_id;
                next_connection_id += 1;

                if let Err(_) = sis.lock().push(SenderInfo {
                    id,
                    role,
                    sender: Some(new_sender),
                }) {
                    next_connection_id -= 1;

                    // TODO: Close the acceptor
                } else {
                    ws_receivers.push(new_receiver).map_err(error::heapless)?;

                    process_initial_response(
                        &mut sender,
                        id,
                        role,
                        &valve_state,
                        &water_meter_state,
                        &battery_state,
                    )
                    .await?;
                }
            }
            SelectResult::Close => break,
            SelectResult::Receive(index, receive) => match receive {
                WebFrame::Request(ref request) => {
                    let (id, role) = {
                        let sender = &sis.lock()[index];

                        (sender.id, sender.role)
                    };

                    process_request(
                        &sis,
                        id,
                        role,
                        request,
                        &mut sender,
                        &mut valve_command,
                        &mut wm_command,
                        &valve_state,
                        &water_meter_state,
                        &battery_state,
                    )
                    .await?;
                }
                WebFrame::Control => (),
                WebFrame::Close | WebFrame::Unknown => {
                    ws_receivers.swap_remove(index);
                    sis.lock().swap_remove(index);
                }
            },
        }
    }

    Ok(())
}

pub async fn run_sender<A, const N: usize>(
    sis: Arc<impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>>,
    mut receiver: impl Receiver<Data = (ConnectionId, WebEvent)>,
    mut valve: impl Receiver<Data = Option<ValveState>>,
    mut wm: impl Receiver<Data = WaterMeterState>,
    mut battery: impl Receiver<Data = BatteryState>,
) -> error::Result<()>
where
    A: Acceptor,
{
    loop {
        let receiver = receiver.recv().fuse();
        let valve = valve.recv().fuse();
        let wm = wm.recv().fuse();
        let battery = battery.recv().fuse();

        pin_mut!(receiver, valve, wm, battery);

        select! {
            state = receiver => { let (id, event) = state?; web_send_single(&sis, id, &event).await?; },
            state = valve => web_send_all(&sis, &WebEvent::ValveState(state?)).await?,
            state = wm => web_send_all(&sis, &WebEvent::WaterMeterState(state?)).await?,
            state = battery => web_send_all(&sis, &WebEvent::BatteryState(state?)).await?,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_request<A, const N: usize>(
    sis: &Arc<impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>>,
    connection_id: ConnectionId,
    role: Role,
    request: &WebRequest,
    sender: &mut impl Sender<Data = (ConnectionId, WebEvent)>,
    valve_command: &mut impl Sender<Data = ValveCommand>,
    wm_command: &mut impl Sender<Data = WaterMeterCommand>,
    valve_state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    water_meter_state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
) -> error::Result<()>
where
    A: Acceptor,
{
    let response = request.response(role);
    let accepted = response.is_accepted();

    if accepted {
        match request.payload() {
            WebRequestPayload::Authenticate(username, password) => {
                if let Some(role) = authenticate(username, password) {
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
                    .await?;
                } else {
                    sender
                        .send((connection_id, WebEvent::AuthenticationFailed))
                        .await?;
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
                    .await?
            }
            WebRequestPayload::ValveCommand(command) => valve_command.send(*command).await?,
            WebRequestPayload::ValveStateRequest => {
                sender
                    .send((connection_id, WebEvent::ValveState(valve_state.get())))
                    .await?
            }
            WebRequestPayload::WaterMeterCommand(command) => wm_command.send(*command).await?,
            WebRequestPayload::WaterMeterStateRequest => {
                sender
                    .send((
                        connection_id,
                        WebEvent::WaterMeterState(water_meter_state.get()),
                    ))
                    .await?
            }
            WebRequestPayload::BatteryStateRequest => {
                sender
                    .send((connection_id, WebEvent::BatteryState(battery_state.get())))
                    .await?
            }
            WebRequestPayload::WifiStatusRequest => todo!(),
        }
    }

    Ok(())
}

async fn process_initial_response(
    sender: &mut impl Sender<Data = (ConnectionId, WebEvent)>,
    connection_id: ConnectionId,
    role: Role,
    valve_state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    water_meter_state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
) -> error::Result<()> {
    let events = [
        WebEvent::RoleState(role),
        WebEvent::ValveState(valve_state.get()),
        WebEvent::WaterMeterState(water_meter_state.get()),
        WebEvent::BatteryState(battery_state.get()),
    ];

    for event in events {
        if role >= event.role() {
            sender.send((connection_id, event)).await?;
        }
    }

    Ok(())
}

fn authenticate(username: &str, password: &str) -> Option<Role> {
    Some(Role::Admin) // TODO
}

async fn web_send_all<A, const N: usize>(
    sis: &Arc<impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>>,
    event: &WebEvent,
) -> error::Result<()>
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
        web_send_single(sis, id, event).await?;
    }

    Ok(())
}

async fn web_send_single<A, const N: usize>(
    sis: &Arc<impl Mutex<Data = heapless::Vec<SenderInfo<A>, N>>>,
    id: ConnectionId,
    event: &WebEvent,
) -> error::Result<()>
where
    A: Acceptor,
{
    let sender = if let Some(si) = sis.lock().iter_mut().find(|si| si.id == id) {
        mem::replace(&mut si.sender, None)
    } else {
        None
    };

    if let Some(mut sender) = sender {
        let result = web_send::<A>(&mut sender, event).await;

        if let Some(si) = sis.lock().iter_mut().find(|si| si.id == id) {
            si.sender = Some(sender);
        }

        result?;
    }

    Ok(())
}

async fn web_send<A>(ws_sender: &mut A::Sender, event: &WebEvent) -> error::Result<()>
where
    A: Acceptor,
{
    let mut frame_buf = [0_u8; 1024];

    let (frame_type, size) = to_ws_frame(event, &mut frame_buf).unwrap();

    ws_sender.send(frame_type, Some(&frame_buf[..size])).await?;

    Ok(())
}

async fn web_receive<A>(
    ws_receiver: &mut A::Receiver,
    index: usize,
) -> error::Result<(usize, WebFrame)>
where
    A: Acceptor,
{
    let mut frame_buf = [0_u8; 1024];

    let (frame_type, size) = ws_receiver.recv(&mut frame_buf).await?;

    let receive = from_ws_frame(frame_type, &frame_buf[..size]);

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

fn to_ws_frame(event: &WebEvent, frame_buf: &mut [u8]) -> error::Result<(FrameType, usize)> {
    let slice = to_slice(event, frame_buf).map_err(|e| anyhow::anyhow!(e))?;

    Ok((FrameType::Binary(false), slice.len()))
}
