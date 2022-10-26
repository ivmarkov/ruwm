use std::rc::Rc;

use yew::prelude::*;
use yewdux_middleware::*;

use ruwm::dto::battery::BatteryState;

#[derive(Default, Clone, Debug, Eq, PartialEq, Store)]
pub struct BatteryStore(pub BatteryState);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatteryMsg(pub BatteryState);

impl Reducer<BatteryStore> for BatteryMsg {
    fn apply(&self, mut store: Rc<BatteryStore>) -> Rc<BatteryStore> {
        let state = Rc::make_mut(&mut store);

        state.0 = self.0.clone();

        store
    }
}

#[function_component(Battery)]
pub fn battery() -> Html {
    let battery_store = use_store::<BatteryStore>();

    html! {
        {format!("Battery Powered: {}, mV: {:?}", battery_store.0.powered.unwrap_or(false), battery_store.0.voltage)}
    }
}
