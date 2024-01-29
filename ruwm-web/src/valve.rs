use std::rc::Rc;

use yew::prelude::*;
use yewdux::prelude::*;

use ruwm::dto::valve::ValveState;

#[derive(Default, Clone, Debug, Eq, PartialEq, Store)]
pub struct ValveStore(pub Option<ValveState>);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValveMsg(pub Option<ValveState>);

impl Reducer<ValveStore> for ValveMsg {
    fn apply(self, mut store: Rc<ValveStore>) -> Rc<ValveStore> {
        let state = Rc::make_mut(&mut store);

        state.0 = self.0;

        store
    }
}

#[function_component(Valve)]
pub fn valve() -> Html {
    let valve_store = use_store_value::<ValveStore>();

    html! {
        {format!("Valve State: {:?}", valve_store.0.as_ref())}
    }
}
