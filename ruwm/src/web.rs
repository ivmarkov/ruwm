use core::cell::Cell;

use embassy_sync::signal::Signal;
use log::info;

use embassy_futures::select::{select, select4};
use embassy_sync::blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex, RawMutex};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::mutex::Mutex as AsyncMutex;

use edge_frame::dto::Role;

use channel_bridge::asynch::*;
use channel_bridge::notification::Notification;

use crate::battery;
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

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum AuthEvent {
    Connected,
    Authenticated(Role),
    AuthenticationFailed,
    LoggedOut,
}

impl AuthEvent {
    pub fn role(&self) -> Role {
        if let Self::Authenticated(role) = self {
            *role
        } else {
            Role::None
        }
    }
}

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
    sender: S,
    receiver: R,
    valve_state_notif: &Notification,
    wm_state_notif: &Notification,
    battery_state_notif: &Notification,
) -> Result<(), R::Error>
where
    S: Sender<Data = WebEvent>,
    R: Receiver<Data = Option<WebRequest>, Error = S::Error>,
{
    let role = Mutex::<NoopRawMutex, _>::new(Cell::new(Role::None));
    let auth_signal = Signal::<CriticalSectionRawMutex, _>::new();

    let sender = AsyncMutex::<NoopRawMutex, _>::new(sender);

    auth_signal.signal(AuthEvent::Connected);

    select(
        receive(receiver, &role, &auth_signal),
        select4(
            process_auth_event(&sender, &auth_signal),
            process_state_update(&sender, &role, &valve::STATE, valve_state_notif, |state| {
                WebEvent::ValveState(state)
            }),
            process_state_update(&sender, &role, &wm::STATE, wm_state_notif, |state| {
                WebEvent::WaterMeterState(state)
            }),
            process_state_update(
                &sender,
                &role,
                &battery::STATE,
                battery_state_notif,
                WebEvent::BatteryState,
            ),
        ),
    )
    .await;

    Ok(())
}

async fn receive<R>(
    mut receiver: R,
    role: &Mutex<impl RawMutex, Cell<Role>>,
    auth_signal: &Signal<CriticalSectionRawMutex, AuthEvent>,
) -> Result<(), R::Error>
where
    R: Receiver<Data = Option<WebRequest>>,
{
    loop {
        let request = receiver.recv().await?;
        info!("[WEB RECEIVE] {:?}", request);

        if let Some(request) = request {
            let new_auth_event = if request.role() <= role.lock(Cell::get) {
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

                            Some(AuthEvent::Authenticated(new_role))
                        } else {
                            info!("[WS] Authentication failed");

                            Some(AuthEvent::AuthenticationFailed)
                        }
                    }
                    WebRequest::Logout => Some(AuthEvent::LoggedOut),
                }
            } else {
                None
            };

            if let Some(new_auth_event) = new_auth_event {
                role.lock(|role| role.set(new_auth_event.role()));
                auth_signal.signal(new_auth_event);
            }
        } else {
            break;
        }
    }

    Ok(())
}

async fn process_auth_event<'a, S>(
    sender: &AsyncMutex<impl RawMutex, S>,
    auth_signal: &Signal<CriticalSectionRawMutex, AuthEvent>,
) -> Result<(), S::Error>
where
    S: Sender<Data = WebEvent>,
{
    loop {
        let event = auth_signal.wait().await;

        let web_event = match event {
            AuthEvent::Authenticated(role) => WebEvent::RoleState(role),
            AuthEvent::AuthenticationFailed => WebEvent::AuthenticationFailed,
            _ => WebEvent::RoleState(Role::None),
        };

        send_event(sender, web_event, event.role()).await?;

        send_event(
            sender,
            WebEvent::ValveState(valve::STATE.get()),
            event.role(),
        )
        .await?;

        send_event(
            sender,
            WebEvent::WaterMeterState(wm::STATE.get()),
            event.role(),
        )
        .await?;

        send_event(
            sender,
            WebEvent::BatteryState(battery::STATE.get()),
            event.role(),
        )
        .await?;
    }
}

async fn process_state_update<'a, S, T>(
    sender: &AsyncMutex<impl RawMutex, S>,
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

        send_event(sender, to_web_event(state.get()), role.lock(Cell::get)).await?;
    }
}

async fn send_event<S>(
    sender: &AsyncMutex<impl RawMutex, S>,
    event: WebEvent,
    role: Role,
) -> Result<(), S::Error>
where
    S: Sender<Data = WebEvent>,
{
    if event.role() <= role {
        info!("[WS SEND] {:?}", event);

        let sender = &mut *sender.lock().await;

        sender.send(event).await
    } else {
        Ok(())
    }
}

fn authenticate(_username: &str, _password: &str) -> Option<Role> {
    Some(Role::Admin) // TODO
}
