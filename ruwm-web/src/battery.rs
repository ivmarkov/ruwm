use ruwm::battery::BatteryState as BatteryValue;

use yew::prelude::*;

use edge_frame::redust::*;

pub type BatteryState = ValueState<BatteryValue>;
pub type BatteryAction = ValueAction<BatteryValue>;

#[derive(Properties, Clone, PartialEq)]
pub struct BatteryProps<R: Reducible2> {
    pub projection: Projection<R, BatteryState, BatteryAction>,
}

#[function_component(Battery)]
pub fn battery<R: Reducible2>(props: &BatteryProps<R>) -> Html {
    let battery_store = use_projection(props.projection.clone());

    html! {
        {format!("Battery Powered: {}, mV: {:?}", battery_store.powered.unwrap_or(false), battery_store.voltage)}
    }
}
