use edge_frame::redust::*;
use edge_frame::role::Credentials;
use edge_frame::role::RoleAction;
use edge_frame::role::RoleStateValue;

use ruwm::dto::web::WebEvent;
use ruwm::dto::web::WebRequest;

use crate::battery::BatteryAction;
use crate::state::*;
use crate::valve::*;

pub fn from_event(store: &UseStoreHandle<AppState>, event: &WebEvent) -> Option<AppAction> {
    match event {
        //WebEvent::Response(_) => todo!(),
        WebEvent::AuthenticationFailed => {
            let credentials = match &**store.role {
                RoleStateValue::Authenticating(credentials) => credentials.clone(),
                _ => Default::default(),
            };

            Some(AppAction::Role(RoleAction::Update(
                RoleStateValue::AuthenticationFailed(credentials),
            )))
        }
        WebEvent::RoleState(value) => Some(AppAction::Role(RoleAction::Update(
            RoleStateValue::Role(*value),
        ))),
        WebEvent::BatteryState(value) => Some(AppAction::Battery(BatteryAction::Update(*value))),
        _ => None,
    }
}

pub fn to_request(action: &AppAction) -> Option<WebRequest> {
    match action {
        AppAction::Role(RoleAction::Update(RoleStateValue::Authenticating(Credentials {
            username,
            password,
        }))) => Some(WebRequest::Authenticate(
            username.as_str().into(),
            password.as_str().into(),
        )),
        AppAction::Role(RoleAction::Update(RoleStateValue::LoggingOut(_))) => {
            Some(WebRequest::Logout)
        }
        AppAction::Valve(ValveAction::Update(value)) => Some(WebRequest::ValveCommand(
            matches!(
                value,
                Some(ruwm::dto::valve::ValveState::Open)
                    | Some(ruwm::dto::valve::ValveState::Opening)
            )
            .then(|| ruwm::dto::valve::ValveCommand::Open)
            .unwrap_or(ruwm::dto::valve::ValveCommand::Close),
        )),
        _ => None,
    }
}
