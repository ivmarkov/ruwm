use core::mem;

extern crate alloc;
use alloc::{sync::Arc, vec::Vec};

use futures::{pin_mut, select, stream::FuturesUnordered, FutureExt, StreamExt};

use postcard::{from_bytes, to_slice};

use embedded_svc::{
    channel::nonblocking::{Receiver, Sender},
    mutex::Mutex,
    utils::rest::role::Role,
    ws::{
        nonblocking::{Acceptor, Receiver as _, Sender as _},
        FrameType,
    },
};

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

pub fn sis<A, M>() -> Arc<M>
where
    A: Acceptor,
    M: Mutex<Data = Vec<SenderInfo<A>>>,
{
    Arc::new(M::new(Vec::new()))
}

#[allow(clippy::too_many_arguments)]
pub async fn run_receiver<A>(
    sis: Arc<impl Mutex<Data = Vec<SenderInfo<A>>>>,
    mut ws_acceptor: A,
    mut sender: impl Sender<Data = (ConnectionId, WebEvent)>,
    mut valve_command: impl Sender<Data = ValveCommand>,
    mut water_meter_command: impl Sender<Data = WaterMeterCommand>,
    valve_state: StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    water_meter_state: StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: StateSnapshot<impl Mutex<Data = BatteryState>>,
) -> error::Result<()>
where
    A: Acceptor,
{
    let mut next_connection_id: ConnectionId = 0;
    let mut ws_receivers = Vec::new();

    loop {
        enum SelectResult<A: Acceptor> {
            Accept(A::Sender, A::Receiver),
            Close,
            Receive(usize, WebFrame),
            Empty,
        }

        let result: SelectResult<A> = {
            let mut ws_receiver = ws_receivers
                .iter_mut()
                .enumerate()
                .map(|(index, ws_receiver)| web_receive::<A>(ws_receiver, index))
                .collect::<FuturesUnordered<_>>();

            let ws_acceptor = ws_acceptor.accept().fuse();
            let ws_receiver = ws_receiver.next().fuse();

            pin_mut!(ws_acceptor, ws_receiver);

            select! {
                accept = ws_acceptor => accept
                    .map_err(error::svc)?
                    .map_or_else(|| SelectResult::Close, |(ws_sender, ws_receiver)| SelectResult::Accept(ws_sender, ws_receiver)),
                ws_receive = ws_receiver => match ws_receive {
                    Some(ws_receive) => ws_receive.map(|(index, receive)| SelectResult::Receive(index, receive))?,
                    None => SelectResult::Empty,
                },
            }
        };

        match result {
            SelectResult::Accept(sender, receiver) => {
                let id = next_connection_id;
                next_connection_id += 1;

                ws_receivers.push(receiver);

                sis.lock().push(SenderInfo {
                    id,
                    role: Role::None,
                    sender: Some(sender),
                });
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
                        &mut water_meter_command,
                        &valve_state,
                        &water_meter_state,
                        &battery_state,
                    )
                    .await?;
                }
                WebFrame::Control => (),
                WebFrame::Close | WebFrame::Unknown => {
                    ws_receivers.remove(index);
                    sis.lock().remove(index);
                }
            },
            SelectResult::Empty => (),
        }
    }

    Ok(())
}

pub async fn run_sender<A>(
    sis: Arc<impl Mutex<Data = Vec<SenderInfo<A>>>>,
    mut receiver: impl Receiver<Data = (ConnectionId, WebEvent)>,
    mut valve_state: impl Receiver<Data = Option<ValveState>>,
    mut water_meter_state: impl Receiver<Data = WaterMeterState>,
    mut battery_state: impl Receiver<Data = BatteryState>,
) -> error::Result<()>
where
    A: Acceptor,
{
    loop {
        let receiver = receiver.recv().fuse();
        let valve_state = valve_state.recv().fuse();
        let water_meter_state = water_meter_state.recv().fuse();
        let battery_state = battery_state.recv().fuse();

        pin_mut!(receiver, valve_state, water_meter_state, battery_state);

        select! {
            state = receiver => { let (id, event) = state?; web_send_single(&sis, id, &event).await?; },
            state = valve_state => web_send_all(&sis, &WebEvent::ValveState(state?)).await?,
            state = water_meter_state => web_send_all(&sis, &WebEvent::WaterMeterState(state?)).await?,
            state = battery_state => web_send_all(&sis, &WebEvent::BatteryState(state?)).await?,
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_request<A>(
    sis: &Arc<impl Mutex<Data = Vec<SenderInfo<A>>>>,
    connection_id: ConnectionId,
    role: Role,
    request: &WebRequest,
    sender: &mut impl Sender<Data = (ConnectionId, WebEvent)>,
    valve_command: &mut impl Sender<Data = ValveCommand>,
    water_meter_command: &mut impl Sender<Data = WaterMeterCommand>,
    valve_state: &StateSnapshot<impl Mutex<Data = Option<ValveState>>>,
    water_meter_state: &StateSnapshot<impl Mutex<Data = WaterMeterState>>,
    battery_state: &StateSnapshot<impl Mutex<Data = BatteryState>>,
) -> error::Result<()>
where
    A: Acceptor,
{
    if let WebRequestPayload::Authenticate(username, password) = request.payload() {
        let response = if let Some(role) = authenticate(username, password) {
            sis.lock()
                .iter_mut()
                .find(|si| si.id == connection_id)
                .unwrap()
                .role = role;

            request.accept()
        } else {
            request.deny()
        };

        sender
            .send((connection_id, WebEvent::Response(response)))
            .await?;
    } else {
        let response = request.response(role);
        let accepted = response.is_accepted();

        sender
            .send((connection_id, WebEvent::Response(response)))
            .await?;

        if accepted {
            match request.payload() {
                WebRequestPayload::Authenticate(_, _) => unreachable!(),
                WebRequestPayload::ValveCommand(command) => valve_command.send(*command).await?,
                WebRequestPayload::ValveStateRequest => {
                    sender
                        .send((connection_id, WebEvent::ValveState(valve_state.get())))
                        .await?
                }
                WebRequestPayload::WaterMeterCommand(command) => {
                    water_meter_command.send(*command).await?
                }
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
    }

    Ok(())
}

fn authenticate(username: &str, password: &str) -> Option<Role> {
    Some(Role::User) // TODO
}

async fn web_send_all<A>(
    sis: &Arc<impl Mutex<Data = Vec<SenderInfo<A>>>>,
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
        .collect::<Vec<_>>();

    for id in ids {
        web_send_single(sis, id, event).await?;
    }

    Ok(())
}

async fn web_send_single<A>(
    sis: &Arc<impl Mutex<Data = Vec<SenderInfo<A>>>>,
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
    if frame_type.is_partial() {
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
