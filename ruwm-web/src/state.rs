use std::rc::Rc;

use yew::prelude::*;

use edge_frame::redust::*;
use edge_frame::role::*;
use edge_frame::wifi::*;

use crate::battery::{BatteryAction, BatteryState};
use crate::valve::{ValveAction, ValveState};

#[derive(Default, Debug, Clone, PartialEq)]
pub struct AppState {
    pub role: Rc<RoleState>,
    pub wifi: Rc<WifiState>,
    pub valve: Rc<ValveState>,
    pub battery: Rc<BatteryState>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            wifi: Rc::new(ValueState::new(Some(Default::default()))),
            ..Default::default()
        }
    }

    pub fn role() -> Projection<AppState, RoleState, RoleAction> {
        Projection::new(|state: &AppState| &*state.role, AppAction::Role)
    }

    pub fn wifi() -> Projection<AppState, WifiState, WifiAction> {
        Projection::new(|state: &AppState| &*state.wifi, AppAction::Wifi)
    }

    pub fn valve() -> Projection<AppState, ValveState, ValveAction> {
        Projection::new(|state: &AppState| &*state.valve, AppAction::Valve)
    }

    pub fn battery() -> Projection<AppState, BatteryState, BatteryAction> {
        Projection::new(|state: &AppState| &*state.battery, AppAction::Battery)
    }
}

impl Reducible for AppState {
    type Action = AppAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        match action {
            AppAction::Role(action) => Self {
                role: self.role.clone().reduce(action),
                ..(&*self).clone()
            },
            AppAction::Wifi(action) => Self {
                wifi: self.wifi.clone().reduce(action),
                ..(&*self).clone()
            },
            AppAction::Battery(action) => Self {
                battery: self.battery.clone().reduce(action),
                ..(&*self).clone()
            },
            AppAction::Valve(action) => Self {
                valve: self.valve.clone().reduce(action),
                ..(&*self).clone()
            },
        }
        .into()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppAction {
    Role(RoleAction),
    Wifi(WifiAction),
    Valve(ValveAction),
    Battery(BatteryAction),
}
