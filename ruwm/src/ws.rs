use embassy_futures::select::select_slice;

use embedded_svc::ws::asynch::server::Acceptor;

use channel_bridge::asynch::{ws, *};
use channel_bridge::notification::Notification;

use crate::web::{self, *};

#[cfg(feature = "ws-max-connections-16")]
pub const WS_MAX_CONNECTIONS: usize = 16;
#[cfg(all(
    feature = "ws-max-connections-8",
    not(feature = "ws-max-connections-16")
))]
pub const WS_MAX_CONNECTIONS: usize = 8;
#[cfg(all(
    feature = "ws-max-connections-4",
    not(any(feature = "ws-max-connections-16", feature = "ws-max-connections-8"))
))]
pub const WS_MAX_CONNECTIONS: usize = 4;
#[cfg(not(any(
    feature = "ws-max-connections-16",
    feature = "ws-max-connections-8",
    feature = "ws-max-connections-4"
)))]
pub const WS_MAX_CONNECTIONS: usize = 2;

pub const WS_MAX_FRAME_LEN: usize = 512;

#[allow(clippy::declare_interior_mutable_const)]
const NOTIF: Notification = Notification::new();

static HANDLERS_VALVE_STATE_NOTIF: [Notification; WS_MAX_CONNECTIONS] = [NOTIF; WS_MAX_CONNECTIONS];
static HANDLERS_WM_STATE_NOTIF: [Notification; WS_MAX_CONNECTIONS] = [NOTIF; WS_MAX_CONNECTIONS];
static HANDLERS_WM_STATS_STATE_NOTIF: [Notification; WS_MAX_CONNECTIONS] =
    [NOTIF; WS_MAX_CONNECTIONS];
static HANDLERS_BATTERY_STATE_NOTIF: [Notification; WS_MAX_CONNECTIONS] =
    [NOTIF; WS_MAX_CONNECTIONS];
static HANDLERS_REMAINING_TIME_STATE_NOTIF: [Notification; WS_MAX_CONNECTIONS] =
    [NOTIF; WS_MAX_CONNECTIONS];
static HANDLERS_MQTT_STATE_NOTIF: [Notification; WS_MAX_CONNECTIONS] = [NOTIF; WS_MAX_CONNECTIONS];
static HANDLERS_WIFI_STATE_NOTIF: [Notification; WS_MAX_CONNECTIONS] = [NOTIF; WS_MAX_CONNECTIONS];

struct WebHandler;

impl ws::AcceptorHandler for WebHandler {
    type SendData = WebEvent;

    type ReceiveData = WebRequest;

    async fn handle<S, R>(&self, sender: S, receiver: R, index: usize) -> Result<(), S::Error>
    where
        S: Sender<Data = Self::SendData>,
        R: Receiver<Error = S::Error, Data = Option<Self::ReceiveData>>,
        S::Error: core::fmt::Debug,
    {
        web::handle(
            sender,
            receiver,
            &HANDLERS_VALVE_STATE_NOTIF[index],
            &HANDLERS_WM_STATE_NOTIF[index],
            &HANDLERS_WM_STATS_STATE_NOTIF[index],
        )
        .await
    }
}

pub async fn process<A: Acceptor>(
    acceptor_svr: &mut ws::Acceptor<WS_MAX_CONNECTIONS, WS_MAX_FRAME_LEN, 1>,
    acceptor: A,
) {
    acceptor_svr.run(acceptor, WebHandler).await;
}

pub async fn handle<S, R>(
    sender: S,
    send_buf: &mut [u8],
    receiver: R,
    recv_buf: &mut [u8],
    index: usize,
) -> Result<(), ws::WsError<S::Error>>
where
    S: embedded_svc::ws::asynch::Sender,
    R: embedded_svc::ws::asynch::Receiver<Error = S::Error>,
{
    web::handle(
        ws::WsSvcSender::new(sender, send_buf),
        ws::WsSvcReceiver::new(receiver, recv_buf),
        &HANDLERS_VALVE_STATE_NOTIF[index],
        &HANDLERS_WM_STATE_NOTIF[index],
        &HANDLERS_WM_STATS_STATE_NOTIF[index],
    )
    .await
}

pub async fn broadcast() {
    let mut notifs = [
        VALVE_STATE_NOTIF.wait(),
        WM_STATE_NOTIF.wait(),
        WM_STATS_STATE_NOTIF.wait(),
        BATTERY_STATE_NOTIF.wait(),
        REMAINING_TIME_STATE_NOTIF.wait(),
        MQTT_STATE_NOTIF.wait(),
        WIFI_STATE_NOTIF.wait(),
    ];

    loop {
        let targets = match select_slice(&mut notifs)
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
