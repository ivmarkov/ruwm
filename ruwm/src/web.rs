use core::cell::Cell;
use core::fmt::{self, Debug, Display};

use log::{info, warn};

use postcard::{from_bytes, to_slice};

use embassy_futures::select::select4;
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::mutex::Mutex as AsyncMutex;

use embedded_svc::ws::asynch::server::Acceptor;
use embedded_svc::ws::{self, FrameType};

use edge_frame::dto::Role;

use crate::battery;
use crate::notification::Notification;
use crate::state::State;
use crate::valve;
use crate::wm;

pub use crate::dto::web::*;

#[cfg(any(
    feature = "ws-max-connections-2",
    not(any(
        feature = "ws-max-connections-4",
        feature = "ws-max-connections-8",
        feature = "ws-max-connections-16"
    ))
))]
pub const WS_MAX_CONNECTIONS: usize = 2;
#[cfg(feature = "ws-max-connections-4")]
pub const WS_MAX_CONNECTIONS: usize = 4;
#[cfg(feature = "ws-max-connections-8")]
pub const WS_MAX_CONNECTIONS: usize = 8;
#[cfg(feature = "ws-max-connections-16")]
pub const WS_MAX_CONNECTIONS: usize = 16;

#[derive(Debug)]
pub enum WebError<E> {
    IoError(E),
    UnknownFrameError,
    PostcardError(postcard::Error),
}

impl<E> Display for WebError<E>
where
    E: Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO Error: {}", e),
            Self::UnknownFrameError => write!(f, "Unknown Frame Error"),
            Self::PostcardError(e) => write!(f, "Postcard Error: {}", e),
        }
    }
}

#[cfg(feature = "std")]
impl<E> std::error::Error for WebError<E> where E: Display + Debug {}

impl<E> From<postcard::Error> for WebError<E> {
    fn from(e: postcard::Error) -> Self {
        WebError::PostcardError(e)
    }
}

#[derive(Debug)]
enum WebFrame {
    Request(WebRequest),
    Control,
    Close,
    Unknown,
}

pub const WS_MAX_FRAME_LEN: usize = 512;

pub struct StateNotifs<const N: usize>([Notification; N]);

impl<const N: usize> StateNotifs<N> {
    pub const fn new() -> Self {
        const NOTIF: Notification = Notification::new();

        Self([NOTIF; N])
    }
}

impl<const N: usize> StateNotifs<N> {
    pub fn as_ref(&self) -> [&Notification; N] {
        self.0
            .iter()
            .collect::<heapless::Vec<_, N>>()
            .into_array::<N>()
            .unwrap_or_else(|_| unreachable!())
    }
}

pub struct StateWebNotifs<const N: usize> {
    pub valve: StateNotifs<N>,
    pub wm: StateNotifs<N>,
    pub wm_stats: StateNotifs<N>,
    pub battery: StateNotifs<N>,
}

impl<const N: usize> StateWebNotifs<N> {
    pub const fn new() -> Self {
        Self {
            valve: StateNotifs::new(),
            wm: StateNotifs::new(),
            wm_stats: StateNotifs::new(),
            battery: StateNotifs::new(),
        }
    }
}

pub static NOTIFY: StateWebNotifs<{ WS_MAX_CONNECTIONS }> = StateWebNotifs::new();

pub async fn process<A: Acceptor, const W: usize>(acceptor: A) {
    info!(
        "Creating queue for {} tasks and {} workers",
        W, WS_MAX_CONNECTIONS
    );
    let channel = embassy_sync::channel::Channel::<NoopRawMutex, _, W>::new();

    let mut workers = heapless::Vec::<_, WS_MAX_CONNECTIONS>::new();

    for index in 0..WS_MAX_CONNECTIONS {
        let channel = &channel;

        workers
            .push({
                async move {
                    loop {
                        let (sender, receiver) = channel.recv().await;

                        info!("Handler {}: Got new connection", index);

                        let res = handle_connection(
                            sender,
                            receiver,
                            &NOTIFY.valve.0[index],
                            &NOTIFY.wm.0[index],
                            &NOTIFY.battery.0[index],
                        )
                        .await;

                        match res {
                            Ok(()) => {
                                info!("Handler {}: connection closed", index);
                            }
                            Err(e) => {
                                warn!("Handler {}: connection closed with error {:?}", index, e);
                            }
                        }
                    }
                }
            })
            .unwrap_or_else(|_| unreachable!());
    }

    let workers = workers
        .into_array::<WS_MAX_CONNECTIONS>()
        .unwrap_or_else(|_| unreachable!());

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
                        warn!("Got error when accepting a new connection: {:?}", e);
                    }
                }
            }
        },
        embassy_futures::select::select_array(workers),
    )
    .await;

    info!("Server processing loop quit");
}

async fn handle_connection<WS, WR>(
    mut sender: WS,
    receiver: WR,
    valve_state_notif: &Notification,
    wm_state_notif: &Notification,
    battery_state_notif: &Notification,
) -> Result<(), WebError<WS::Error>>
where
    WR: ws::asynch::Receiver,
    WS: ws::asynch::Sender<Error = WR::Error>,
{
    let role = Role::None;

    web_send(&mut sender, &WebEvent::RoleState(role)).await?;

    let role = Mutex::<NoopRawMutex, _>::new(Cell::new(role));
    let sender = AsyncMutex::<NoopRawMutex, _>::new(sender);

    select4(
        receive(receiver, &sender, &role),
        send_state(&sender, &role, &valve::STATE, valve_state_notif, |state| {
            WebEvent::ValveState(state)
        }),
        send_state(&sender, &role, &wm::STATE, wm_state_notif, |state| {
            WebEvent::WaterMeterState(state)
        }),
        send_state(
            &sender,
            &role,
            &battery::STATE,
            battery_state_notif,
            |state| WebEvent::BatteryState(state),
        ),
    )
    .await;

    Ok(())
}

async fn receive<WS, WR>(
    mut receiver: WR,
    sender: &AsyncMutex<impl RawMutex, WS>,
    role: &Mutex<impl RawMutex, Cell<Role>>,
) -> Result<(), WebError<WS::Error>>
where
    WR: ws::asynch::Receiver,
    WS: ws::asynch::Sender<Error = WR::Error>,
{
    loop {
        let request = match web_receive(&mut receiver).await? {
            WebFrame::Request(request) => request,
            WebFrame::Control => todo!(),
            WebFrame::Close => break,
            WebFrame::Unknown => return Err(WebError::UnknownFrameError),
        };

        let response = request.response(role.lock(|role| role.get()));

        let web_event = if response.is_accepted() {
            match request.payload() {
                WebRequestPayload::ValveCommand(command) => {
                    valve::COMMAND.signal(*command);
                    WebEvent::Response(response)
                }
                WebRequestPayload::WaterMeterCommand(command) => {
                    wm::COMMAND.signal(*command);
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
                WebRequestPayload::ValveStateRequest => WebEvent::ValveState(valve::STATE.get()),
                WebRequestPayload::WaterMeterStateRequest => {
                    WebEvent::WaterMeterState(wm::STATE.get())
                }
                WebRequestPayload::BatteryStateRequest => {
                    WebEvent::BatteryState(battery::STATE.get())
                }
                WebRequestPayload::WifiStatusRequest => todo!(),
            }
        } else {
            WebEvent::Response(response)
        };

        web_send(&mut *sender.lock().await, &web_event).await?;
    }

    Ok(())
}

async fn send_state<'a, S, T, const N: usize>(
    connection: &AsyncMutex<impl RawMutex, S>,
    role: &Mutex<impl RawMutex, Cell<Role>>,
    state: &State<'a, T, N>,
    state_notif: &Notification,
    to_web_event: impl Fn(T) -> WebEvent,
) -> Result<(), WebError<S::Error>>
where
    S: ws::asynch::Sender,
    T: Clone,
{
    loop {
        state_notif.wait().await;

        web_send_auth(
            &mut *connection.lock().await,
            &to_web_event(state.get()),
            role.lock(|role| role.get()),
        )
        .await?;
    }
}

fn authenticate(_username: &str, _password: &str) -> Option<Role> {
    Some(Role::Admin) // TODO
}

async fn web_send_auth<S>(
    ws_sender: S,
    event: &WebEvent,
    role: Role,
) -> Result<(), WebError<S::Error>>
where
    S: ws::asynch::Sender,
{
    if event.role() >= role {
        web_send(ws_sender, event).await
    } else {
        Ok(())
    }
}

async fn web_send<S>(mut ws_sender: S, event: &WebEvent) -> Result<(), WebError<S::Error>>
where
    S: ws::asynch::Sender,
{
    info!("[WS SEND] {:?}", event);

    let mut frame_buf = [0_u8; WS_MAX_FRAME_LEN];

    let (frame_type, size) = to_ws_frame(event, &mut frame_buf)?;

    ws_sender
        .send(frame_type, &frame_buf[..size])
        .await
        .map_err(WebError::IoError)?;

    Ok(())
}

async fn web_receive<R>(mut ws_receiver: R) -> Result<WebFrame, WebError<R::Error>>
where
    R: ws::asynch::Receiver,
{
    let mut frame_buf = [0_u8; WS_MAX_FRAME_LEN];

    let (frame_type, size) = ws_receiver
        .recv(&mut frame_buf)
        .await
        .map_err(WebError::IoError)?;

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
