use core::cell::Cell;
use core::fmt::Debug;
use core::future::Future;

use log::info;

use embassy_futures::select::select4;
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::mutex::Mutex as AsyncMutex;

use edge_frame::dto::Role;

use crate::battery;
use crate::notification::Notification;
use crate::state::State;
use crate::valve;
use crate::wm;

pub use crate::dto::web::*;

pub trait WebSender {
    type Error: Debug;

    type SendFuture<'a>: Future<Output = Result<(), Self::Error>>
    where
        Self: 'a;

    fn send<'a>(&'a mut self, event: &'a WebEvent) -> Self::SendFuture<'a>;
}

impl<'t, T> WebSender for &'t mut T
where
    T: WebSender + 't,
{
    type Error = T::Error;

    type SendFuture<'a> = impl Future<Output = Result<(), Self::Error>> where Self: 'a;

    fn send<'a>(&'a mut self, event: &'a WebEvent) -> Self::SendFuture<'a> {
        async move { (*self).send(event).await }
    }
}

pub trait WebReceiver {
    type Error: Debug;

    type RecvFuture<'a>: Future<Output = Result<Option<WebRequest>, Self::Error>>
    where
        Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_>;
}

impl<'t, T> WebReceiver for &'t mut T
where
    T: WebReceiver + 't,
{
    type Error = T::Error;

    type RecvFuture<'a> = impl Future<Output = Result<Option<WebRequest>, Self::Error>> where Self: 'a;

    fn recv(&mut self) -> Self::RecvFuture<'_> {
        async move { (*self).recv().await }
    }
}

pub(crate) static VALVE_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WM_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WM_STATS_STATE_NOTIF: Notification = Notification::new();
pub(crate) static BATTERY_STATE_NOTIF: Notification = Notification::new();
pub(crate) static REMAINING_TIME_STATE_NOTIF: Notification = Notification::new();
pub(crate) static MQTT_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WIFI_STATE_NOTIF: Notification = Notification::new();

pub async fn process<WS, WR>(sender: WS, receiver: WR)
where
    WR: WebReceiver,
    WS: WebSender<Error = WR::Error>,
{
    handle(
        sender,
        receiver,
        &VALVE_STATE_NOTIF,
        &WM_STATE_NOTIF,
        &BATTERY_STATE_NOTIF,
    )
    .await
    .unwrap();
}

pub async fn handle<WS, WR>(
    mut sender: WS,
    receiver: WR,
    valve_state_notif: &Notification,
    wm_state_notif: &Notification,
    battery_state_notif: &Notification,
) -> Result<(), WR::Error>
where
    WR: WebReceiver,
    WS: WebSender<Error = WR::Error>,
{
    let role = Role::None;

    let event = WebEvent::RoleState(role);

    info!("[WEB SEND] {:?}", event);
    sender.send(&event).await?;

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
) -> Result<(), WR::Error>
where
    WR: WebReceiver,
    WS: WebSender<Error = WR::Error>,
{
    loop {
        let request = receiver.recv().await?;
        info!("[WEB RECEIVE] {:?}", request);

        if let Some(request) = request {
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
                    WebRequestPayload::ValveStateRequest => {
                        WebEvent::ValveState(valve::STATE.get())
                    }
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

            {
                let sender = &mut *sender.lock().await;

                info!("[WS SEND] {:?}", web_event);
                sender.send(&web_event).await?;
            }
        } else {
            break;
        }
    }

    Ok(())
}

async fn send_state<'a, S, T>(
    connection: &AsyncMutex<impl RawMutex, S>,
    role: &Mutex<impl RawMutex, Cell<Role>>,
    state: &State<'a, T>,
    state_notif: &Notification,
    to_web_event: impl Fn(T) -> WebEvent,
) -> Result<(), S::Error>
where
    S: WebSender,
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

async fn web_send_auth<S>(mut ws_sender: S, event: &WebEvent, role: Role) -> Result<(), S::Error>
where
    S: WebSender,
{
    if event.role() >= role {
        info!("[WS SEND] {:?}", event);

        ws_sender.send(event).await
    } else {
        Ok(())
    }
}
