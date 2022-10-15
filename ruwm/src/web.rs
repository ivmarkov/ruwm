use core::cell::Cell;

use log::info;

use embassy_futures::select::select4;
use embassy_sync::blocking_mutex::raw::{NoopRawMutex, RawMutex};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::mutex::Mutex as AsyncMutex;

use edge_frame::dto::Role;

use channel_bridge::asynch::*;

use crate::battery;
use crate::notification::Notification;
use crate::state::State;
use crate::valve;
use crate::wm;

pub use crate::dto::web::*;

pub(crate) static VALVE_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WM_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WM_STATS_STATE_NOTIF: Notification = Notification::new();
pub(crate) static BATTERY_STATE_NOTIF: Notification = Notification::new();
pub(crate) static REMAINING_TIME_STATE_NOTIF: Notification = Notification::new();
pub(crate) static MQTT_STATE_NOTIF: Notification = Notification::new();
pub(crate) static WIFI_STATE_NOTIF: Notification = Notification::new();

pub async fn process<S, R>(sender: S, receiver: R)
where
    S: Sender<Data = WebEvent>,
    R: Receiver<Data = Option<WebRequest>, Error = S::Error>,
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

pub async fn handle<S, R>(
    mut sender: S,
    receiver: R,
    valve_state_notif: &Notification,
    wm_state_notif: &Notification,
    battery_state_notif: &Notification,
) -> Result<(), R::Error>
where
    S: Sender<Data = WebEvent>,
    R: Receiver<Data = Option<WebRequest>, Error = S::Error>,
{
    let role = Role::None;

    let event = WebEvent::RoleState(role);

    info!("[WEB SEND] {:?}", event);
    sender.send(event).await?;

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

async fn receive<S, R>(
    mut receiver: R,
    sender: &AsyncMutex<impl RawMutex, S>,
    role: &Mutex<impl RawMutex, Cell<Role>>,
) -> Result<(), R::Error>
where
    S: Sender<Data = WebEvent>,
    R: Receiver<Data = Option<WebRequest>, Error = S::Error>,
{
    loop {
        let request = receiver.recv().await?;
        info!("[WEB RECEIVE] {:?}", request);

        if let Some(request) = request {
            let web_event = if request.role() >= role.lock(|role| role.get()) {
                match request {
                    WebRequest::ValveCommand(command) => {
                        valve::COMMAND.signal(command);
                        None
                    }
                    WebRequest::WaterMeterCommand(command) => {
                        wm::COMMAND.signal(command);
                        None
                    }
                    WebRequest::Authenticate(username, password) => {
                        if let Some(new_role) = authenticate(&username, &password) {
                            info!("[S] Authenticated; role: {}", new_role);

                            role.lock(|role| role.set(new_role));
                            Some(WebEvent::RoleState(new_role))
                        } else {
                            info!("[WS] Authentication failed");

                            role.lock(|role| role.set(Role::None));
                            Some(WebEvent::AuthenticationFailed)
                        }
                    }
                    WebRequest::Logout => {
                        role.lock(|role| role.set(Role::None));
                        Some(WebEvent::RoleState(Role::None))
                    }
                    WebRequest::ValveStateRequest => Some(WebEvent::ValveState(valve::STATE.get())),
                    WebRequest::WaterMeterStateRequest => {
                        Some(WebEvent::WaterMeterState(wm::STATE.get()))
                    }
                    WebRequest::BatteryStateRequest => {
                        Some(WebEvent::BatteryState(battery::STATE.get()))
                    }
                    WebRequest::WifiStatusRequest => todo!(),
                }
            } else {
                Some(WebEvent::NoPermissions)
            };

            if let Some(web_event) = web_event {
                let sender = &mut *sender.lock().await;

                info!("[WS SEND] {:?}", web_event);
                sender.send(web_event).await?;
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
    S: Sender<Data = WebEvent>,
    T: Clone,
{
    loop {
        state_notif.wait().await;

        web_send_auth(
            &mut *connection.lock().await,
            to_web_event(state.get()),
            role.lock(|role| role.get()),
        )
        .await?;
    }
}

fn authenticate(_username: &str, _password: &str) -> Option<Role> {
    Some(Role::Admin) // TODO
}

async fn web_send_auth<S>(mut ws_sender: S, event: WebEvent, role: Role) -> Result<(), S::Error>
where
    S: Sender<Data = WebEvent>,
{
    if event.role() >= role {
        info!("[WS SEND] {:?}", event);

        ws_sender.send(event).await
    } else {
        Ok(())
    }
}
