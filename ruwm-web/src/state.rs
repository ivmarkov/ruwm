use std::rc::Rc;

use yew::Reducible;

use crate::battery::{BatteryAction, BatteryStore};

pub enum GlobalAction {
    Battery(BatteryAction),
}

pub struct GlobalStore {
    battery: Rc<BatteryStore>,
}

impl Reducible for GlobalStore {
    type Action = GlobalAction;

    fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
        let this = match action {
            GlobalAction::Battery(action) => Self {
                battery: self.battery.clone().reduce(action),
                ..*self
            },
        };

        this.into()
    }
}
