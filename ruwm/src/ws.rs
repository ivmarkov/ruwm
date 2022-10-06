use core::fmt::{self, Debug, Display};
use core::future::Future;

use log::{info, warn};

use embassy_futures::select::select_array;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

use embedded_svc::ws::asynch::server::Acceptor;
use embedded_svc::ws::{self, FrameType};

use crate::notification::Notification;
use crate::web;

use crate::web::*;

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
pub enum WsError<E> {
    IoError(E),
    UnknownFrameError,
    PostcardError(postcard::Error),
}

impl<E> Display for WsError<E>
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
impl<E> std::error::Error for WsError<E> where E: Display + Debug {}

impl<E> From<postcard::Error> for WsError<E> {
    fn from(e: postcard::Error) -> Self {
        WsError::PostcardError(e)
    }
}

pub struct WsReceiver<R>(R);

impl<R> WebReceiver for WsReceiver<R>
where
    R: ws::asynch::Receiver,
{
    type Error = WsError<R::Error>;

    type RecvFuture<'a> = impl Future<Output = Result<Option<WebRequest>, Self::Error>> where Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move {
            let mut frame_buf = [0_u8; WS_MAX_FRAME_LEN];

            let (frame_type, frame_buf) = loop {
                let (frame_type, size) = self
                    .0
                    .recv(&mut frame_buf)
                    .await
                    .map_err(WsError::IoError)?;

                if frame_type != FrameType::Ping && frame_type != FrameType::Pong {
                    break (frame_type, &frame_buf[..size]);
                }
            };

            match frame_type {
                FrameType::Text(_) | FrameType::Continue(_) => Err(WsError::UnknownFrameError),
                FrameType::Binary(_) => Ok(Some(
                    postcard::from_bytes(frame_buf).map_err(WsError::PostcardError)?,
                )),
                FrameType::Close | FrameType::SocketClose => Ok(None),
                _ => unreachable!(),
            }
        }
    }
}

pub struct WsSender<S>(S);

impl<S> WebSender for WsSender<S>
where
    S: ws::asynch::Sender,
{
    type Error = WsError<S::Error>;

    type SendFuture<'a> = impl Future<Output = Result<(), Self::Error>> where Self: 'a;

    fn send<'a>(&'a mut self, event: &'a WebEvent) -> Self::SendFuture<'a> {
        async move {
            let mut frame_buf = [0_u8; WS_MAX_FRAME_LEN];

            let frame_data = postcard::to_slice(event, &mut frame_buf)?;

            self.0
                .send(FrameType::Binary(false), frame_data)
                .await
                .map_err(WsError::IoError)?;

            Ok(())
        }
    }
}

pub const WS_MAX_FRAME_LEN: usize = 512;

const NOTIF: Notification = Notification::new();

static HANDLERS_VALVE_STATE_NOTIF: [Notification; WS_MAX_FRAME_LEN] = [NOTIF; WS_MAX_FRAME_LEN];
static HANDLERS_WM_STATE_NOTIF: [Notification; WS_MAX_FRAME_LEN] = [NOTIF; WS_MAX_FRAME_LEN];
static HANDLERS_WM_STATS_STATE_NOTIF: [Notification; WS_MAX_FRAME_LEN] = [NOTIF; WS_MAX_FRAME_LEN];
static HANDLERS_BATTERY_STATE_NOTIF: [Notification; WS_MAX_FRAME_LEN] = [NOTIF; WS_MAX_FRAME_LEN];
static HANDLERS_REMAINING_TIME_STATE_NOTIF: [Notification; WS_MAX_FRAME_LEN] =
    [NOTIF; WS_MAX_FRAME_LEN];
static HANDLERS_MQTT_STATE_NOTIF: [Notification; WS_MAX_FRAME_LEN] = [NOTIF; WS_MAX_FRAME_LEN];
static HANDLERS_WIFI_STATE_NOTIF: [Notification; WS_MAX_FRAME_LEN] = [NOTIF; WS_MAX_FRAME_LEN];

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

                        let res = web::handle(
                            WsSender(sender),
                            WsReceiver(receiver),
                            &HANDLERS_VALVE_STATE_NOTIF[index],
                            &HANDLERS_WM_STATE_NOTIF[index],
                            &HANDLERS_BATTERY_STATE_NOTIF[index],
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

    embassy_futures::select::select3(
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
        broadcast(),
        embassy_futures::select::select_array(workers),
    )
    .await;

    info!("Server processing loop quit");
}

async fn broadcast() {
    loop {
        let targets = match select_array([
            VALVE_STATE_NOTIF.wait(),
            WM_STATE_NOTIF.wait(),
            WM_STATS_STATE_NOTIF.wait(),
            BATTERY_STATE_NOTIF.wait(),
            REMAINING_TIME_STATE_NOTIF.wait(),
            MQTT_STATE_NOTIF.wait(),
            WIFI_STATE_NOTIF.wait(),
        ])
        .await
        .1
        {
            0 => &HANDLERS_VALVE_STATE_NOTIF,
            1 => &HANDLERS_WM_STATE_NOTIF,
            2 => &HANDLERS_WM_STATS_STATE_NOTIF,
            3 => &HANDLERS_BATTERY_STATE_NOTIF,
            4 => &HANDLERS_REMAINING_TIME_STATE_NOTIF,
            5 => &HANDLERS_MQTT_STATE_NOTIF,
            6 => &HANDLERS_WIFI_STATE_NOTIF,
            _ => unreachable!(),
        };

        for target in targets {
            target.notify();
        }
    }
}
